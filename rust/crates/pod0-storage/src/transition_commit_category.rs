use pod0_application::{
    ActivitySubject, RequestDisposition, RequestRejectionReason, UserArtifactActivityInput,
    UserArtifactMutation, UserArtifactTransition, plan_user_artifact_activity,
};
use pod0_domain::{
    CategoryId, CategoryOrigin, CommandId, MAX_CATEGORIES, StateRevision,
    UnixTimestampMilliseconds, validate_category, validate_color_hex,
};

use super::TransitionCommit;
use super::application_support::{fingerprint, legacy_library_receipt, next_core_revision};
use crate::{CategoryEdit, StorageError, TransitionIngress, TransitionIngressKind};

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_category_create(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    name: &str,
    description: &str,
    color_hex: Option<&str>,
    origin: CategoryOrigin,
    observed_at_ms: i64,
) -> Result<(StateRevision, CategoryId), StorageError> {
    let category_id = CategoryId::from_bytes(command_id.into_bytes());
    let receipt = commit(
        path,
        command_id,
        command_fingerprint,
        observed_at_ms,
        |transaction| {
            let invalid = validate_category(name, description, color_hex, origin).is_err()
                || crate::category_store_read::active_category_count(transaction)?
                    >= MAX_CATEGORIES;
            Ok(invalid.then_some(RequestRejectionReason::Invalid))
        },
        |transaction| {
            crate::library_store_categories::create_category_in_transaction(
                transaction,
                command_id,
                command_fingerprint,
                category_id,
                name,
                description,
                color_hex,
                origin,
                observed_at_ms,
            )
        },
    )?;
    Ok((receipt, category_id))
}

pub(crate) fn commit_category_update(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    category_id: CategoryId,
    edit: &CategoryEdit,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    commit(
        path,
        command_id,
        command_fingerprint,
        observed_at_ms,
        |transaction| {
            if !crate::category_store_read::category_exists(transaction, category_id)? {
                return Ok(Some(RequestRejectionReason::MissingSubject));
            }
            Ok(invalid_edit(edit).then_some(RequestRejectionReason::Invalid))
        },
        |transaction| {
            crate::library_store_categories::update_category_in_transaction(
                transaction,
                command_id,
                command_fingerprint,
                category_id,
                edit,
                observed_at_ms,
            )
        },
    )
}

pub(crate) fn commit_category_delete(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    category_id: CategoryId,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    commit(
        path,
        command_id,
        command_fingerprint,
        observed_at_ms,
        |transaction| {
            Ok((!crate::category_store_read::category_exists(transaction, category_id)?)
                .then_some(RequestRejectionReason::MissingSubject))
        },
        |transaction| {
            crate::library_store_categories::delete_category_in_transaction(
                transaction,
                command_id,
                command_fingerprint,
                category_id,
                observed_at_ms,
            )
        },
    )
}

fn commit(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    observed_at_ms: i64,
    validate: impl FnOnce(
        &rusqlite::Transaction<'_>,
    ) -> Result<Option<RequestRejectionReason>, StorageError>,
    apply: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<StateRevision, StorageError>,
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
            let current = crate::category_store_read::collection_revision(transaction)?;
            let committed = next_core_revision(transaction, "read category core revision")?;
            let legacy = legacy_library_receipt(
                transaction,
                command_id,
                command_fingerprint,
                "read category command receipt",
            )?;
            let rejection = if legacy.is_some() {
                None
            } else {
                validate(transaction)?
            };
            let disposition = match (legacy, rejection) {
                (Some(_), _) => RequestDisposition::Duplicate,
                (_, Some(reason)) => RequestDisposition::Rejected { reason },
                _ => RequestDisposition::Accepted,
            };
            plan_user_artifact_activity(UserArtifactActivityInput {
                command_id,
                subject: ActivitySubject::Operation { command_id },
                episode_ids: Vec::new(),
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
                plan.map_mutation(|mutation| {
                    (mutation, committed, legacy.unwrap_or(current))
                })
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, (mutation, committed, return_revision)| match mutation {
            UserArtifactMutation::Apply => {
                require_revision(transaction, expected)?;
                let actual = apply(transaction)?;
                (actual == committed)
                    .then_some(actual)
                    .ok_or(StorageError::RevisionConflict)
            }
            UserArtifactMutation::None => {
                require_revision(transaction, expected)?;
                Ok(return_revision)
            }
        },
    )?;
    match receipt.disposition {
        RequestDisposition::Accepted | RequestDisposition::Duplicate => {
            Ok(receipt.committed_revision)
        }
        RequestDisposition::Rejected {
            reason: RequestRejectionReason::MissingSubject,
        } => Err(StorageError::EntityNotFound),
        RequestDisposition::Rejected { .. } => Err(StorageError::InvalidCategory),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn invalid_edit(edit: &CategoryEdit) -> bool {
    edit.name
        .as_ref()
        .is_some_and(|value| value.trim().is_empty() || value.len() > 128)
        || edit
            .description
            .as_ref()
            .is_some_and(|value| value.trim().is_empty() || value.len() > 1_024)
        || validate_color_hex(edit.color_hex.as_deref()).is_err()
}

fn require_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (crate::category_store_read::collection_revision(transaction)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}
