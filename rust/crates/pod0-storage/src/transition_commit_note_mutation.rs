use pod0_application::{
    RequestDisposition, RequestRejectionReason, UserArtifactActivityInput, UserArtifactMutation,
    UserArtifactTransition, plan_user_artifact_activity,
};
use pod0_domain::{
    CommandId, NoteId, NoteKind, NoteRevision, NoteTarget, StateRevision, UnixTimestampMilliseconds,
};

use super::TransitionCommit;
use super::application_support::fingerprint;
use super::note_support::require_revision;
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

#[path = "transition_commit_note_mutation_preflight.rs"]
mod preflight;
use preflight::preflight;

pub(super) enum NoteWrite<'a> {
    Update {
        note_id: NoteId,
        expected: NoteRevision,
        text: &'a str,
        kind: NoteKind,
        target: Option<NoteTarget>,
    },
    SetDeleted {
        note_id: NoteId,
        expected: NoteRevision,
        deleted: bool,
    },
    Clear {
        expected: StateRevision,
    },
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_note_update(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    note_id: NoteId,
    expected: NoteRevision,
    text: &str,
    kind: NoteKind,
    target: Option<NoteTarget>,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    commit_note_write(
        path,
        command_id,
        command_fingerprint,
        NoteWrite::Update {
            note_id,
            expected,
            text,
            kind,
            target,
        },
        observed_at_ms,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_note_deleted(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    note_id: NoteId,
    expected: NoteRevision,
    deleted: bool,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    commit_note_write(
        path,
        command_id,
        command_fingerprint,
        NoteWrite::SetDeleted {
            note_id,
            expected,
            deleted,
        },
        observed_at_ms,
    )
}

pub(crate) fn commit_note_clear(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    expected: StateRevision,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    commit_note_write(
        path,
        command_id,
        command_fingerprint,
        NoteWrite::Clear { expected },
        observed_at_ms,
    )
}

fn commit_note_write(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    write: NoteWrite<'_>,
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
            let (current, committed, legacy, subject, episode_ids, rejection) =
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
                episode_ids,
                current_revision: current,
                committed_revision: if disposition == RequestDisposition::Accepted {
                    committed
                } else {
                    current
                },
                transition: UserArtifactTransition::NoteChanged,
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
                let revision = apply(
                    transaction,
                    command_id,
                    command_fingerprint,
                    &write,
                    observed_at_ms,
                )?;
                if revision != committed {
                    return Err(StorageError::RevisionConflict);
                }
                Ok(revision)
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
    write: &NoteWrite<'_>,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    match write {
        NoteWrite::Update {
            note_id,
            expected,
            text,
            kind,
            target,
        } => crate::library_store_notes::update_note_in_transaction(
            transaction,
            command_id,
            fingerprint,
            *note_id,
            *expected,
            text,
            *kind,
            *target,
            observed_at_ms,
        ),
        NoteWrite::SetDeleted {
            note_id,
            expected,
            deleted,
        } => crate::library_store_notes::set_note_deleted_in_transaction(
            transaction,
            command_id,
            fingerprint,
            *note_id,
            *expected,
            *deleted,
            observed_at_ms,
        ),
        NoteWrite::Clear { expected } => crate::library_store_notes::clear_notes_in_transaction(
            transaction,
            command_id,
            fingerprint,
            *expected,
            observed_at_ms,
        ),
    }
}

fn rejection_error(reason: RequestRejectionReason) -> StorageError {
    match reason {
        RequestRejectionReason::MissingSubject => StorageError::EntityNotFound,
        RequestRejectionReason::RevisionConflict => StorageError::RevisionConflict,
        _ => StorageError::InvalidNote,
    }
}
