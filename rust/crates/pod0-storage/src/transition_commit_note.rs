use pod0_application::{
    NoteCreateActivityInput, NoteCreateMutation, RequestDisposition, RequestRejectionReason,
    plan_note_create,
};
use pod0_domain::{
    CommandId, NoteAuthor, NoteId, NoteKind, NoteTarget, StateRevision, UnixTimestampMilliseconds,
    validate_new_note,
};

use super::TransitionCommit;
use super::application_support::{fingerprint, legacy_library_receipt};
use super::note_support::{require_revision, revisions};
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_note_create(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    text: &str,
    kind: NoteKind,
    author: NoteAuthor,
    target: Option<NoteTarget>,
    observed_at_ms: i64,
) -> Result<(StateRevision, NoteId), StorageError> {
    let note_id = NoteId::from_bytes(command_id.into_bytes());
    let episode_id = match target {
        Some(NoteTarget::Episode { episode_id, .. }) => Some(episode_id),
        _ => None,
    };
    let ingress = TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: command_id.into_bytes(),
            fingerprint: fingerprint(command_fingerprint)?,
        };
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        UnixTimestampMilliseconds::new(observed_at_ms),
        |transaction| {
            let (current, committed) = revisions(transaction)?;
            let legacy = legacy_library_receipt(
                transaction,
                command_id,
                command_fingerprint,
                "read note command receipt",
            )?;
            let invalid = validate_new_note(text, kind, author, target).is_err()
                || !crate::library_store_note_support::target_reference_is_valid(
                    transaction, note_id, target,
                )?;
            let disposition = if legacy.is_some() {
                RequestDisposition::Duplicate
            } else if invalid {
                RequestDisposition::Rejected {
                    reason: RequestRejectionReason::Invalid,
                }
            } else {
                RequestDisposition::Accepted
            };
            plan_note_create(NoteCreateActivityInput {
                command_id,
                note_id,
                episode_id,
                current_revision: current,
                committed_revision: if disposition == RequestDisposition::Accepted {
                    committed
                } else {
                    current
                },
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
            (NoteCreateMutation::Apply, committed, _) => {
                require_revision(transaction, expected)?;
                let (revision, created) =
                    crate::library_store_note_create::create_note_in_transaction(
                        transaction,
                        command_id,
                        note_id,
                        command_fingerprint,
                        text,
                        kind,
                        author,
                        target,
                        observed_at_ms,
                    )?;
                if created != note_id || revision != committed {
                    return Err(StorageError::RevisionConflict);
                }
                Ok(revision)
            }
            (NoteCreateMutation::None, _, return_revision) => {
                require_revision(transaction, expected)?;
                Ok(return_revision)
            }
        },
    )?;
    match receipt.disposition {
        RequestDisposition::Accepted | RequestDisposition::Duplicate => {
            Ok((receipt.committed_revision, note_id))
        }
        RequestDisposition::Rejected { .. } => Err(StorageError::InvalidNote),
        _ => Err(StorageError::InvalidActivity),
    }
}
