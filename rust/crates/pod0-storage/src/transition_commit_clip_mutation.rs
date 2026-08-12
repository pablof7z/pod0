use pod0_application::{
    RequestDisposition, RequestRejectionReason, UserArtifactActivityInput, UserArtifactMutation,
    UserArtifactTransition, plan_user_artifact_activity,
};
use pod0_domain::{
    ClipId, ClipRevision, CommandId, SpeakerId, StateRevision, UnixTimestampMilliseconds,
};

use super::TransitionCommit;
use super::application_support::fingerprint;
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

#[path = "transition_commit_clip_mutation_preflight.rs"]
mod preflight;
use preflight::preflight;

pub(super) enum ClipWrite<'a> {
    Update {
        clip_id: ClipId,
        expected: ClipRevision,
        start: u64,
        end: u64,
        caption: Option<&'a str>,
        speaker_id: Option<SpeakerId>,
        frozen_text: &'a str,
    },
    SetDeleted {
        clip_id: ClipId,
        expected: ClipRevision,
        deleted: bool,
    },
    Clear {
        expected: StateRevision,
    },
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_clip_update(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    clip_id: ClipId,
    expected: ClipRevision,
    start: u64,
    end: u64,
    caption: Option<&str>,
    speaker_id: Option<SpeakerId>,
    frozen_text: &str,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    commit(
        path,
        command_id,
        command_fingerprint,
        ClipWrite::Update {
            clip_id,
            expected,
            start,
            end,
            caption,
            speaker_id,
            frozen_text,
        },
        observed_at_ms,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_clip_deleted(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    clip_id: ClipId,
    expected: ClipRevision,
    deleted: bool,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    commit(
        path,
        command_id,
        command_fingerprint,
        ClipWrite::SetDeleted {
            clip_id,
            expected,
            deleted,
        },
        observed_at_ms,
    )
}

pub(crate) fn commit_clip_clear(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    expected: StateRevision,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    commit(
        path,
        command_id,
        command_fingerprint,
        ClipWrite::Clear { expected },
        observed_at_ms,
    )
}

fn commit(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    write: ClipWrite<'_>,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let ingress = TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: command_id.into_bytes(),
            fingerprint: fingerprint(command_fingerprint)?,
        };
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        UnixTimestampMilliseconds::new(observed_at_ms),
        |transaction| {
            let (current, committed, legacy, subject, episodes, rejection) =
                preflight(transaction, command_id, command_fingerprint, &write)?;
            let disposition = if legacy.is_some() {
                RequestDisposition::Duplicate
            } else if let Some(reason) = rejection {
                RequestDisposition::Rejected { reason }
            } else {
                RequestDisposition::Accepted
            };
            plan_user_artifact_activity(UserArtifactActivityInput {
                command_id,
                subject,
                episode_ids: episodes,
                current_revision: current,
                committed_revision: if disposition == RequestDisposition::Accepted {
                    committed
                } else {
                    current
                },
                transition: UserArtifactTransition::ClipChanged,
                disposition,
            })
            .map(|plan| {
                plan.map_mutation(|mutation| {
                    (mutation, committed, legacy.unwrap_or(current))
                })
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation {
            (UserArtifactMutation::Apply, committed, _) => {
                require_revision(transaction, expected)?;
                let actual = apply(
                    transaction,
                    command_id,
                    command_fingerprint,
                    &write,
                    observed_at_ms,
                )?;
                if actual != committed {
                    return Err(StorageError::RevisionConflict);
                }
                Ok(actual)
            }
            (UserArtifactMutation::None, _, return_revision) => {
                require_revision(transaction, expected)?;
                Ok(return_revision)
            }
        },
    )?;
    match receipt.disposition {
        RequestDisposition::Accepted | RequestDisposition::Duplicate => {
            Ok(receipt.committed_revision)
        }
        RequestDisposition::Rejected { reason } => Err(rejection_error(reason)),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn apply(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    write: &ClipWrite<'_>,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    match write {
        ClipWrite::Update {
            clip_id,
            expected,
            start,
            end,
            caption,
            speaker_id,
            frozen_text,
        } => crate::library_store_clip_mutation::update_clip_in_transaction(
            transaction,
            command_id,
            fingerprint,
            *clip_id,
            *expected,
            *start,
            *end,
            *caption,
            *speaker_id,
            frozen_text,
            observed_at_ms,
        ),
        ClipWrite::SetDeleted {
            clip_id,
            expected,
            deleted,
        } => crate::library_store_clip_mutation::set_clip_deleted_in_transaction(
            transaction,
            command_id,
            fingerprint,
            *clip_id,
            *expected,
            *deleted,
            observed_at_ms,
        ),
        ClipWrite::Clear { expected } => {
            crate::library_store_clip_mutation::clear_clips_in_transaction(
                transaction,
                command_id,
                fingerprint,
                *expected,
                observed_at_ms,
            )
        }
    }
}

fn require_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    if crate::library_store_clip_support::collection_revision(transaction)? == expected {
        Ok(())
    } else {
        Err(StorageError::RevisionConflict)
    }
}

fn rejection_error(reason: RequestRejectionReason) -> StorageError {
    match reason {
        RequestRejectionReason::MissingSubject => StorageError::EntityNotFound,
        RequestRejectionReason::RevisionConflict => StorageError::RevisionConflict,
        _ => StorageError::InvalidClip,
    }
}
