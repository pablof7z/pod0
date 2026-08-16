use pod0_application::{
    RequestDisposition, RequestRejectionReason, SpeakerActivityInput, SpeakerMutation,
    plan_speaker_activity,
};
use pod0_domain::{
    CommandId, SpeakerEntityId, SpeakerId, StateRevision, TranscriptArtifactId,
    UnixTimestampMilliseconds,
};

use super::TransitionCommit;
use super::application_support::{fingerprint, legacy_library_receipt, next_core_revision};
use crate::speaker_store_model::SpeakerAssignmentOrigin;
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

#[path = "transition_commit_speaker_preflight.rs"]
mod preflight;
use preflight::preflight;

pub(super) enum SpeakerWrite<'a> {
    Create {
        entity_id: SpeakerEntityId,
        display_name: &'a str,
    },
    Rename {
        entity_id: SpeakerEntityId,
        expected_entity_revision: u64,
        display_name: &'a str,
    },
    Assign {
        artifact_id: TranscriptArtifactId,
        speaker_id: SpeakerId,
        entity_id: SpeakerEntityId,
        origin: SpeakerAssignmentOrigin,
        confidence: Option<f64>,
    },
}

pub(crate) fn commit_speaker_create(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    entity_id: SpeakerEntityId,
    display_name: &str,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    commit_speaker_write(
        path,
        command_id,
        command_fingerprint,
        SpeakerWrite::Create {
            entity_id,
            display_name,
        },
        observed_at_ms,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_speaker_rename(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    entity_id: SpeakerEntityId,
    expected_entity_revision: u64,
    display_name: &str,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    commit_speaker_write(
        path,
        command_id,
        command_fingerprint,
        SpeakerWrite::Rename {
            entity_id,
            expected_entity_revision,
            display_name,
        },
        observed_at_ms,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_speaker_assignment(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    artifact_id: TranscriptArtifactId,
    speaker_id: SpeakerId,
    entity_id: SpeakerEntityId,
    origin: SpeakerAssignmentOrigin,
    confidence: Option<f64>,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    commit_speaker_write(
        path,
        command_id,
        command_fingerprint,
        SpeakerWrite::Assign {
            artifact_id,
            speaker_id,
            entity_id,
            origin,
            confidence,
        },
        observed_at_ms,
    )
}

fn commit_speaker_write(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    write: SpeakerWrite<'_>,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let ingress = TransitionIngress {
        kind: TransitionIngressKind::ApplicationCommand,
        id: command_id.into_bytes(),
        fingerprint: fingerprint(command_fingerprint)?,
    };
    let receipt = TransitionCommit::open(path)?
        .commit_planned_with(
            ingress,
            UnixTimestampMilliseconds::new(observed_at_ms.max(0)),
            |transaction| {
                let current = core_revision(transaction)?;
                let committed = next_core_revision(transaction, "read speaker core revision")?;
                let legacy = legacy_library_receipt(
                    transaction,
                    command_id,
                    command_fingerprint,
                    "read speaker command receipt",
                )?;
                let details = preflight(transaction, &write, observed_at_ms)?;
                let disposition = if legacy.is_some() {
                    RequestDisposition::Duplicate
                } else {
                    details.disposition
                };
                plan_speaker_activity(SpeakerActivityInput {
                    command_id,
                    actor: details.actor,
                    origin: details.origin,
                    subject: details.subject,
                    episode_id: details.episode_id,
                    current_revision: current,
                    committed_revision: if disposition == RequestDisposition::Accepted {
                        committed
                    } else {
                        current
                    },
                    transition: details.transition,
                    disposition,
                })
                .map(|plan| {
                    plan.map_mutation(|mutation| (mutation, committed, legacy.unwrap_or(current)))
                })
                .map_err(|_| StorageError::InvalidActivity)
            },
            |transaction, expected, (mutation, committed, return_revision)| match mutation {
                SpeakerMutation::Apply => {
                    require_core_revision(transaction, expected)?;
                    apply(
                        transaction,
                        command_id,
                        command_fingerprint,
                        &write,
                        observed_at_ms,
                    )?;
                    let actual = crate::library_store::finish_command(
                        transaction,
                        command_id,
                        command_fingerprint,
                        observed_at_ms,
                    )?;
                    (actual == committed)
                        .then_some(actual)
                        .ok_or(StorageError::RevisionConflict)
                }
                SpeakerMutation::None => {
                    require_core_revision(transaction, expected)?;
                    Ok(return_revision)
                }
            },
        )
        .map_err(|error| match error {
            StorageError::ActivityCommandConflict => StorageError::CommandConflict,
            other => other,
        })?;
    match receipt.disposition {
        RequestDisposition::Accepted
        | RequestDisposition::Duplicate
        | RequestDisposition::NoSemanticChange => Ok(receipt.committed_revision),
        RequestDisposition::Rejected {
            reason: RequestRejectionReason::MissingSubject,
        } => Err(StorageError::EntityNotFound),
        RequestDisposition::Rejected {
            reason: RequestRejectionReason::RevisionConflict,
        } => Err(StorageError::RevisionConflict),
        RequestDisposition::Rejected { .. } => Err(StorageError::InvalidSpeakerEntity),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn apply(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    _fingerprint: &str,
    write: &SpeakerWrite<'_>,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    match write {
        SpeakerWrite::Create {
            entity_id,
            display_name,
        } => crate::speaker_store_write::create_speaker_entity_in_transaction(
            transaction,
            command_id,
            *entity_id,
            display_name,
            observed_at_ms,
        ),
        SpeakerWrite::Rename {
            entity_id,
            expected_entity_revision,
            display_name,
        } => crate::speaker_store_write::rename_speaker_entity_in_transaction(
            transaction,
            *entity_id,
            *expected_entity_revision,
            display_name,
            observed_at_ms,
        ),
        SpeakerWrite::Assign {
            artifact_id,
            speaker_id,
            entity_id,
            origin,
            confidence,
        } => crate::speaker_store_write::assign_speaker_in_transaction(
            transaction,
            command_id,
            *artifact_id,
            *speaker_id,
            *entity_id,
            *origin,
            *confidence,
            observed_at_ms,
        ),
    }
}

fn core_revision(transaction: &rusqlite::Transaction<'_>) -> Result<StateRevision, StorageError> {
    let value: i64 = transaction
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read speaker core revision", error))?;
    super::application_support::revision(value)
}

fn require_core_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (core_revision(transaction)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}
