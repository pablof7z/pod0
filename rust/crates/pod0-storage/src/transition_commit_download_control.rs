use pod0_application::{
    DownloadControlActivityInput, DownloadControlMutation, DownloadControlOperation,
    RequestDisposition, RequestRejectionReason, plan_download_control,
};
use pod0_domain::{CommandId, ContentDigest, EpisodeId, StateRevision, UnixTimestampMilliseconds};

use super::TransitionCommit;
use super::application_support::legacy_library_receipt;
use crate::download_store_cancel::{apply_download_cancel, apply_download_remove};
use crate::download_store_read::workflow;
use crate::{
    DownloadRemovalInput, DownloadWorkflowRecord, DownloadWorkflowTransition, StorageError,
    StoredDownloadStage, TransitionIngress, TransitionIngressKind,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_download_cancel(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    episode_id: EpisodeId,
    expected_revision: StateRevision,
    issued_revision: StateRevision,
    now_ms: i64,
) -> Result<DownloadWorkflowTransition, StorageError> {
    let (receipt, existing) = commit_control(
        path,
        command_id,
        command_fingerprint,
        episode_id,
        DownloadControlOperation::Cancel,
        now_ms,
        |existing| {
            control_rejection(existing, expected_revision, |record| {
                !matches!(
                    record.stage,
                    StoredDownloadStage::Succeeded | StoredDownloadStage::Removing
                )
            })
        },
        |existing| {
            existing
                .filter(|record| record.attempt_id.is_some())
                .map(|record| {
                    crate::download_effect_request::cancel(record, command_id, issued_revision)
                        .map(|request| pod0_application::DownloadEffectAuthorization { request })
                })
                .transpose()
        },
        |transaction| {
            apply_download_cancel(
                transaction,
                command_id,
                command_fingerprint,
                episode_id,
                expected_revision,
                issued_revision,
                now_ms,
            )
        },
    )?;
    finish_control(path, episode_id, receipt, existing)
}

pub(crate) fn commit_download_remove(
    path: &std::path::Path,
    input: DownloadRemovalInput,
) -> Result<DownloadWorkflowTransition, StorageError> {
    let command_id = input.command_id;
    let command_fingerprint = input.command_fingerprint.clone();
    let episode_id = input.episode_id;
    let now_ms = input.now_ms;
    let rejection_input = input.clone();
    let effect_input = input.clone();
    let (receipt, existing) = commit_control(
        path,
        command_id,
        &command_fingerprint,
        episode_id,
        DownloadControlOperation::Remove,
        now_ms,
        move |existing| {
            let mut rejection =
                control_rejection(existing, rejection_input.expected_revision, |record| {
                    matches!(
                        record.stage,
                        StoredDownloadStage::Succeeded | StoredDownloadStage::Failed
                    )
                });
            if rejection.is_none() && existing.is_some_and(|record| record.artifact_key.is_none()) {
                rejection = Some(RequestRejectionReason::MissingPrerequisite);
            }
            rejection
        },
        move |existing| {
            existing
                .map(|record| {
                    crate::download_effect_request::remove(record, &effect_input)
                        .map(|request| pod0_application::DownloadEffectAuthorization { request })
                })
                .transpose()
        },
        |transaction| apply_download_remove(transaction, input),
    )?;
    finish_control(path, episode_id, receipt, existing)
}

#[allow(clippy::too_many_arguments)]
fn commit_control(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    episode_id: EpisodeId,
    operation: DownloadControlOperation,
    now_ms: i64,
    reject: impl FnOnce(Option<&DownloadWorkflowRecord>) -> Option<RequestRejectionReason>,
    authorize_effect: impl FnOnce(
        Option<&DownloadWorkflowRecord>,
    ) -> Result<
        Option<pod0_application::DownloadEffectAuthorization>,
        StorageError,
    >,
    mutate: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<DownloadWorkflowTransition, StorageError>,
) -> Result<(crate::CommitReceipt, Option<DownloadWorkflowRecord>), StorageError> {
    let prior = std::cell::RefCell::new(None);
    let ingress = TransitionIngress {
        kind: TransitionIngressKind::ApplicationCommand,
        id: command_id.into_bytes(),
        fingerprint: fingerprint(command_fingerprint)?,
    };
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        UnixTimestampMilliseconds::new(now_ms),
        |transaction| {
            let existing = workflow(transaction, episode_id)?;
            let legacy = legacy_library_receipt(
                transaction,
                command_id,
                command_fingerprint,
                "read download control command",
            )
            .map_err(download_conflict)?
            .is_some();
            let current = existing
                .as_ref()
                .map_or(StateRevision::INITIAL, |record| record.workflow_revision);
            let rejection = if legacy {
                None
            } else {
                reject(existing.as_ref())
            };
            let effect = if !legacy && rejection.is_none() {
                authorize_effect(existing.as_ref())?
            } else {
                None
            };
            *prior.borrow_mut() = existing;
            plan_download_control(DownloadControlActivityInput {
                command_id,
                episode_id,
                current_revision: current,
                legacy_replay: legacy,
                operation,
                rejection,
                effect,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation {
            DownloadControlMutation::Apply => {
                let actual = workflow(transaction, episode_id)?
                    .map_or(StateRevision::INITIAL, |record| record.workflow_revision);
                if actual != expected {
                    return Err(StorageError::RevisionConflict);
                }
                let revision = mutate(transaction)?.record.workflow_revision;
                super::download::retire_download_effects(transaction, episode_id)?;
                Ok(revision)
            }
            DownloadControlMutation::RecordRejection => {
                let actual = workflow(transaction, episode_id)?
                    .map_or(StateRevision::INITIAL, |record| record.workflow_revision);
                if actual != expected {
                    return Err(StorageError::RevisionConflict);
                }
                Ok(expected)
            }
            DownloadControlMutation::LegacyDuplicate => Ok(expected),
        },
    )?;
    Ok((receipt, prior.borrow_mut().take()))
}

fn finish_control(
    path: &std::path::Path,
    episode_id: EpisodeId,
    receipt: crate::CommitReceipt,
    prior: Option<DownloadWorkflowRecord>,
) -> Result<DownloadWorkflowTransition, StorageError> {
    match receipt.disposition {
        RequestDisposition::Accepted | RequestDisposition::Duplicate => {
            let record = crate::LibraryStore::open_authoritative(path)?
                .download_workflow(episode_id)?
                .ok_or(StorageError::DownloadWorkflowNotFound)?;
            Ok(DownloadWorkflowTransition {
                record,
                replaced: (!receipt.replayed
                    && receipt.disposition == RequestDisposition::Accepted)
                    .then(|| prior.map(Box::new))
                    .flatten(),
            })
        }
        RequestDisposition::Rejected { reason } => Err(rejection_error(Some(reason))),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn control_rejection(
    existing: Option<&DownloadWorkflowRecord>,
    expected: StateRevision,
    allowed: impl FnOnce(&DownloadWorkflowRecord) -> bool,
) -> Option<RequestRejectionReason> {
    match existing {
        None => Some(RequestRejectionReason::MissingSubject),
        Some(record) if record.workflow_revision != expected || !allowed(record) => {
            Some(RequestRejectionReason::RevisionConflict)
        }
        Some(_) => None,
    }
}

fn rejection_error(reason: Option<RequestRejectionReason>) -> StorageError {
    match reason {
        Some(RequestRejectionReason::MissingSubject) => StorageError::DownloadWorkflowNotFound,
        Some(RequestRejectionReason::MissingPrerequisite) => StorageError::InvalidDownloadArtifact,
        _ => StorageError::DownloadWorkflowConflict,
    }
}

fn download_conflict(error: StorageError) -> StorageError {
    match error {
        StorageError::CommandConflict => StorageError::DownloadCommandConflict,
        other => other,
    }
}

fn fingerprint(value: &str) -> Result<ContentDigest, StorageError> {
    if value.len() != 64 {
        return Err(StorageError::DownloadCommandConflict);
    }
    let mut bytes = [0_u8; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| StorageError::DownloadCommandConflict)?;
    }
    Ok(ContentDigest::from_bytes(bytes))
}
