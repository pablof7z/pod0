use std::cell::Cell;

use pod0_application::{
    ActivitySubject, RequestDisposition, RequestRejectionReason, UserArtifactActivityInput,
    UserArtifactMutation, UserArtifactTransition, plan_user_artifact_activity,
};
use pod0_domain::{
    CategoryId, CategoryItemKind, CommandId, EpisodeId, LibraryItemId, StateRevision,
    UnixTimestampMilliseconds,
};

use super::TransitionCommit;
use super::application_support::{fingerprint, legacy_library_receipt, next_core_revision};
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

struct TagMutation {
    action: UserArtifactMutation,
    committed: StateRevision,
    return_revision: StateRevision,
    add: Vec<(LibraryItemId, CategoryItemKind)>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_category_tag(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    category_id: CategoryId,
    add: &[LibraryItemId],
    remove: &[LibraryItemId],
    resolve: impl Fn(LibraryItemId) -> Option<CategoryItemKind>,
    observed_at_ms: i64,
) -> Result<(StateRevision, usize, usize), StorageError> {
    let counts = Cell::new((0, 0));
    let ingress = TransitionIngress {
        kind: TransitionIngressKind::ApplicationCommand,
        id: command_id.into_bytes(),
        fingerprint: fingerprint(command_fingerprint)?,
    };
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        UnixTimestampMilliseconds::new(observed_at_ms),
        |transaction| {
            let current = crate::category_store_read::collection_revision(transaction)?;
            let committed = next_core_revision(transaction, "read category tag core revision")?;
            let legacy = legacy_library_receipt(
                transaction,
                command_id,
                command_fingerprint,
                "read category tag command receipt",
            )?;
            let exists = crate::category_store_read::category_exists(transaction, category_id)?;
            let mut resolved = Vec::with_capacity(add.len());
            let mut missing = false;
            for item in add {
                match resolve(*item) {
                    Some(kind) => resolved.push((*item, kind)),
                    None => missing = true,
                }
            }
            let disposition = if legacy.is_some() {
                RequestDisposition::Duplicate
            } else if !exists || missing {
                RequestDisposition::Rejected {
                    reason: RequestRejectionReason::MissingSubject,
                }
            } else {
                RequestDisposition::Accepted
            };
            let mut episodes = resolved
                .iter()
                .filter_map(|(item, kind)| episode(*item, *kind))
                .collect::<Vec<_>>();
            episodes.extend(
                remove
                    .iter()
                    .filter_map(|item| resolve(*item).and_then(|kind| episode(*item, kind))),
            );
            plan_user_artifact_activity(UserArtifactActivityInput {
                command_id,
                subject: ActivitySubject::Operation { command_id },
                episode_ids: episodes,
                current_revision: current,
                committed_revision: if disposition == RequestDisposition::Accepted {
                    committed
                } else {
                    current
                },
                transition: UserArtifactTransition::CategoryChanged,
                disposition,
            })
            .map(|plan| {
                plan.map_mutation(|action| TagMutation {
                    action,
                    committed,
                    return_revision: legacy.unwrap_or(current),
                    add: resolved,
                })
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation.action {
            UserArtifactMutation::Apply => {
                require_revision(transaction, expected)?;
                let (actual, added, removed) =
                    crate::library_store_category_members::tag_category_items_in_transaction(
                        transaction,
                        command_id,
                        command_fingerprint,
                        category_id,
                        &mutation.add,
                        remove,
                        observed_at_ms,
                    )?;
                if actual != mutation.committed {
                    return Err(StorageError::RevisionConflict);
                }
                counts.set((added, removed));
                Ok(actual)
            }
            UserArtifactMutation::None => {
                require_revision(transaction, expected)?;
                Ok(mutation.return_revision)
            }
        },
    )?;
    match receipt.disposition {
        RequestDisposition::Accepted | RequestDisposition::Duplicate => {
            let (added, removed) = counts.get();
            Ok((receipt.committed_revision, added, removed))
        }
        RequestDisposition::Rejected { .. } => Err(StorageError::EntityNotFound),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn episode(item: LibraryItemId, kind: CategoryItemKind) -> Option<EpisodeId> {
    (kind == CategoryItemKind::Episode).then(|| EpisodeId::from_bytes(item.into_bytes()))
}

fn require_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (crate::category_store_read::collection_revision(transaction)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}
