use pod0_application::{
    ChapterModelExecutionRequest, ChapterModelResponseFormat, DurableModelChapterAction,
    DurableModelChapterEffectRequest, DurablePublisherChapterEffectRequest,
    MAX_PUBLISHER_CHAPTER_DOCUMENT_BYTES,
};
use pod0_domain::{
    CancellationId, CommandId, EpisodeId, HostRequestId, StateRevision, UnixTimestampMilliseconds,
};

use crate::{ModelChapterWorkflowRecord, StorageError};

#[allow(clippy::too_many_arguments)]
pub(crate) fn publisher_request(
    request_id: HostRequestId,
    command_id: CommandId,
    cancellation_id: CancellationId,
    issued_revision: StateRevision,
    deadline_at: Option<UnixTimestampMilliseconds>,
    episode_id: EpisodeId,
    source_url: String,
    not_before: Option<UnixTimestampMilliseconds>,
) -> DurablePublisherChapterEffectRequest {
    DurablePublisherChapterEffectRequest {
        request_id,
        command_id,
        cancellation_id,
        issued_revision,
        deadline_at,
        episode_id,
        source_url,
        not_before,
        maximum_response_bytes: MAX_PUBLISHER_CHAPTER_DOCUMENT_BYTES as u64,
    }
}

pub(crate) fn model_execution_request(
    record: &ModelChapterWorkflowRecord,
) -> Result<DurableModelChapterEffectRequest, StorageError> {
    let active = record
        .active_request
        .as_ref()
        .ok_or(StorageError::ChapterWorkflowConflict)?;
    let response_format = match active.response_format_code {
        1 => ChapterModelResponseFormat::JsonObject,
        _ => return Err(StorageError::ChapterWorkflowConflict),
    };
    Ok(DurableModelChapterEffectRequest {
        request_id: record
            .request_id
            .ok_or(StorageError::ChapterWorkflowConflict)?,
        command_id: record.command_id,
        cancellation_id: record.cancellation_id,
        issued_revision: record.issued_revision,
        deadline_at: record.deadline_at_ms.map(UnixTimestampMilliseconds::new),
        episode_id: record.episode_id,
        generation: record.generation,
        submission_fence_id: record
            .submission_fence_id
            .ok_or(StorageError::ChapterWorkflowConflict)?,
        action: DurableModelChapterAction::Execute {
            execution: ChapterModelExecutionRequest {
                provider: active.provider.clone(),
                model: active.model.clone(),
                system_prompt: active.system_prompt.clone(),
                user_prompt: active.user_prompt.clone(),
                response_format,
                maximum_completion_bytes: active.maximum_completion_bytes,
            },
        },
    })
}

pub(crate) fn model_recovery_request(
    record: &ModelChapterWorkflowRecord,
    provider_operation_id: String,
    provider_status: Option<String>,
) -> Result<DurableModelChapterEffectRequest, StorageError> {
    let active = record
        .active_request
        .as_ref()
        .ok_or(StorageError::ChapterWorkflowConflict)?;
    Ok(DurableModelChapterEffectRequest {
        request_id: record
            .request_id
            .ok_or(StorageError::ChapterWorkflowConflict)?,
        command_id: record.command_id,
        cancellation_id: record.cancellation_id,
        issued_revision: record.issued_revision,
        deadline_at: None,
        episode_id: record.episode_id,
        generation: record.generation,
        submission_fence_id: record
            .submission_fence_id
            .ok_or(StorageError::ChapterWorkflowConflict)?,
        action: DurableModelChapterAction::Recover {
            provider: active.provider.clone(),
            model: active.model.clone(),
            provider_operation_id,
            provider_status,
            maximum_completion_bytes: active.maximum_completion_bytes,
        },
    })
}
