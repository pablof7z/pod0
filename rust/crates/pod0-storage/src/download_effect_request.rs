use pod0_application::{DurableDownloadEffectAction, DurableDownloadEffectRequest};
use pod0_domain::{StateRevision, UnixTimestampMilliseconds, download_attempt_identity};

use crate::download_store_request::{derived_request_id, download_start_request_id};
use crate::{
    DownloadEnsureInput, DownloadFailureInput, DownloadRemovalInput, DownloadWorkflowRecord,
    StorageError,
};

pub(crate) fn start_for_ensure(
    existing: Option<&DownloadWorkflowRecord>,
    input: &DownloadEnsureInput,
) -> Result<DurableDownloadEffectRequest, StorageError> {
    let attempt = existing
        .filter(|record| record.intent_id == input.intent_id)
        .map_or(Ok(1), |record| {
            record
                .attempt
                .checked_add(1)
                .ok_or(StorageError::DownloadWorkflowConflict)
        })?;
    start(
        input.episode_id,
        input.intent_id,
        attempt,
        input.input_version.clone(),
        input.enclosure_url.clone(),
        None,
        input.command_id,
        input.cancellation_id,
        input.issued_revision,
        None,
        input.deadline_at_ms,
    )
}

pub(crate) fn start_for_existing(
    current: &DownloadWorkflowRecord,
    issued_revision: StateRevision,
    not_before_ms: Option<i64>,
    deadline_at_ms: i64,
) -> Result<DurableDownloadEffectRequest, StorageError> {
    let attempt = current
        .attempt
        .checked_add(1)
        .ok_or(StorageError::DownloadWorkflowConflict)?;
    start(
        current.episode_id,
        current.intent_id,
        attempt,
        current.input_version.clone(),
        current.enclosure_url.clone(),
        current.resume_key.clone(),
        current.command_id,
        current.cancellation_id,
        issued_revision,
        not_before_ms,
        deadline_at_ms,
    )
}

pub(crate) fn retry(
    current: &DownloadWorkflowRecord,
    input: &DownloadFailureInput,
) -> Result<DurableDownloadEffectRequest, StorageError> {
    start_for_existing(
        current,
        input.issued_revision,
        input.retry_at_ms,
        input
            .retry_deadline_at_ms
            .ok_or(StorageError::DownloadWorkflowConflict)?,
    )
}

pub(crate) fn cancel(
    current: &DownloadWorkflowRecord,
    command_id: pod0_domain::CommandId,
    issued_revision: StateRevision,
) -> Result<DurableDownloadEffectRequest, StorageError> {
    let attempt_id = current
        .attempt_id
        .ok_or(StorageError::DownloadWorkflowConflict)?;
    Ok(DurableDownloadEffectRequest {
        request_id: derived_request_id(
            b"pod0-download-cancel-request-v1",
            &attempt_id.into_bytes(),
            current.workflow_revision.value,
        ),
        command_id,
        cancellation_id: current.cancellation_id,
        issued_revision,
        not_before: None,
        deadline_at: None,
        action: DurableDownloadEffectAction::Cancel {
            episode_id: current.episode_id,
            intent_id: current.intent_id,
            attempt_id,
            external_task_key: current.external_task_key.clone(),
        },
    })
}

pub(crate) fn remove(
    current: &DownloadWorkflowRecord,
    input: &DownloadRemovalInput,
) -> Result<DurableDownloadEffectRequest, StorageError> {
    let artifact_key = current
        .artifact_key
        .clone()
        .ok_or(StorageError::InvalidDownloadArtifact)?;
    Ok(DurableDownloadEffectRequest {
        request_id: derived_request_id(
            b"pod0-download-remove-request-v1",
            artifact_key.as_bytes(),
            current.workflow_revision.value,
        ),
        command_id: input.command_id,
        cancellation_id: current.cancellation_id,
        issued_revision: input.issued_revision,
        not_before: None,
        deadline_at: Some(UnixTimestampMilliseconds::new(input.deadline_at_ms)),
        action: DurableDownloadEffectAction::Remove {
            episode_id: current.episode_id,
            artifact_key,
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn start(
    episode_id: pod0_domain::EpisodeId,
    intent_id: pod0_domain::DownloadIntentId,
    attempt: u16,
    input_version: String,
    enclosure_url: String,
    resume_key: Option<String>,
    command_id: pod0_domain::CommandId,
    cancellation_id: pod0_domain::CancellationId,
    issued_revision: StateRevision,
    not_before_ms: Option<i64>,
    deadline_at_ms: i64,
) -> Result<DurableDownloadEffectRequest, StorageError> {
    let attempt_id = download_attempt_identity(intent_id, attempt)
        .ok_or(StorageError::DownloadWorkflowConflict)?;
    Ok(DurableDownloadEffectRequest {
        request_id: download_start_request_id(attempt_id),
        command_id,
        cancellation_id,
        issued_revision,
        not_before: not_before_ms.map(UnixTimestampMilliseconds::new),
        deadline_at: Some(UnixTimestampMilliseconds::new(deadline_at_ms)),
        action: DurableDownloadEffectAction::Start {
            episode_id,
            intent_id,
            attempt_id,
            input_version,
            enclosure_url,
            resume_key,
        },
    })
}
