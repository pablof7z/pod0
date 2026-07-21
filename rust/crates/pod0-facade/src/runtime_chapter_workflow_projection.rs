use pod0_application::{
    ChapterModelFailureClassification, ChapterModelRetryDisposition, ChapterWorkflowsProjection,
    CoreFailureCode, ModelChapterWorkflowFailure, ModelChapterWorkflowMode,
    ModelChapterWorkflowProjection, PublisherChapterWorkflowFailure,
    PublisherChapterWorkflowFailureCode, PublisherChapterWorkflowProjection,
    PublisherChapterWorkflowStage, model_chapter_allowed_actions,
};
use pod0_domain::{EpisodeId, UnixTimestampMilliseconds};
use pod0_storage::{
    ChapterWorkflowRecord, ModelChapterWorkflowRecord, PublisherChapterWorkflowRecord,
    PublisherChapterWorkflowState,
};

use crate::runtime_chapter_model_mapping::{failure_code as model_failure_code, model_stage};
use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn chapter_workflows_projection(
        &self,
        episode_id: Option<EpisodeId>,
        offset: usize,
        item_limit: usize,
    ) -> ChapterWorkflowsProjection {
        let Some(store) = &self.store else {
            return ChapterWorkflowsProjection {
                publisher: Vec::new(),
                model: Vec::new(),
                has_more: false,
                failure: Some(failure(CoreFailureCode::StorageUnavailable)),
            };
        };
        let page = match store.chapter_workflow_page(
            episode_id,
            u32::try_from(offset).unwrap_or(u32::MAX),
            u16::try_from(item_limit).unwrap_or(pod0_application::MAX_PROJECTION_ITEMS),
        ) {
            Ok(page) => page,
            Err(_) => {
                return ChapterWorkflowsProjection {
                    publisher: Vec::new(),
                    model: Vec::new(),
                    has_more: false,
                    failure: Some(failure(CoreFailureCode::StorageUnavailable)),
                };
            }
        };
        let mut publisher = Vec::new();
        let mut model = Vec::new();
        for item in page.items {
            match item {
                ChapterWorkflowRecord::Publisher(record) => {
                    publisher.push(publisher_projection(*record));
                }
                ChapterWorkflowRecord::Model(record) => model.push(model_projection(*record)),
            }
        }
        ChapterWorkflowsProjection {
            publisher,
            model,
            has_more: page.has_more,
            failure: None,
        }
    }
}

fn model_projection(record: ModelChapterWorkflowRecord) -> ModelChapterWorkflowProjection {
    let stage = model_stage(record.state);
    let classification = record
        .failure_code
        .as_deref()
        .map(|code| projected_model_classification(code, record.may_have_submitted));
    let failure = classification.map(|value| ModelChapterWorkflowFailure {
        code: value.code,
        safe_detail: record.failure_detail.clone(),
        retry: value.retry,
        may_have_submitted: value.may_have_submitted,
    });
    ModelChapterWorkflowProjection {
        episode_id: record.episode_id,
        configured_model: record.desired_configured_model,
        mode: record
            .active_request
            .as_ref()
            .map(|request| match request.mode {
                pod0_storage::ModelChapterWorkflowMode::Generate => {
                    ModelChapterWorkflowMode::Generate
                }
                pod0_storage::ModelChapterWorkflowMode::Enrich => ModelChapterWorkflowMode::Enrich,
            }),
        source_version: record.active_request.map(|request| request.source_version),
        stage,
        workflow_revision: record.workflow_revision,
        generation: record.generation,
        attempt: record.attempt,
        max_attempts: record.max_attempts,
        request_id: record.request_id,
        cancellation_id: record.cancellation_id,
        not_before: record.not_before_ms.map(UnixTimestampMilliseconds::new),
        selected_artifact_id: record.selected_artifact_id,
        failure,
        replan_pending: record.replan_pending,
        may_have_submitted: record.may_have_submitted,
        created_at: UnixTimestampMilliseconds::new(record.created_at_ms),
        updated_at: UnixTimestampMilliseconds::new(record.updated_at_ms),
        allowed_actions: model_chapter_allowed_actions(stage, classification),
    }
}

fn projected_model_classification(
    code: &str,
    may_have_submitted: bool,
) -> ChapterModelFailureClassification {
    let code = model_failure_code(code);
    let retry = match code {
        pod0_application::ModelChapterWorkflowFailureCode::RateLimited => {
            ChapterModelRetryDisposition::AutomaticRequest
        }
        pod0_application::ModelChapterWorkflowFailureCode::StaleTranscript
        | pod0_application::ModelChapterWorkflowFailureCode::StalePublisherBase
        | pod0_application::ModelChapterWorkflowFailureCode::SelectionChanged => {
            ChapterModelRetryDisposition::Replan
        }
        pod0_application::ModelChapterWorkflowFailureCode::InvalidRequest
        | pod0_application::ModelChapterWorkflowFailureCode::ProviderRejected
        | pod0_application::ModelChapterWorkflowFailureCode::RetryExhausted
        | pod0_application::ModelChapterWorkflowFailureCode::Cancelled => {
            ChapterModelRetryDisposition::Never
        }
        _ => ChapterModelRetryDisposition::ExplicitOnly,
    };
    ChapterModelFailureClassification {
        code,
        retry,
        may_have_submitted,
        resubmission_is_safe: !may_have_submitted,
    }
}

fn publisher_projection(
    record: PublisherChapterWorkflowRecord,
) -> PublisherChapterWorkflowProjection {
    let stage = match record.state {
        PublisherChapterWorkflowState::Requested => PublisherChapterWorkflowStage::Requested,
        PublisherChapterWorkflowState::RetryScheduled => {
            PublisherChapterWorkflowStage::RetryScheduled
        }
        PublisherChapterWorkflowState::Failed => PublisherChapterWorkflowStage::Failed,
        PublisherChapterWorkflowState::Cancelled => PublisherChapterWorkflowStage::Cancelled,
        PublisherChapterWorkflowState::Succeeded => PublisherChapterWorkflowStage::Succeeded,
        PublisherChapterWorkflowState::SourceAbsent => {
            PublisherChapterWorkflowStage::Unsupported { wire_code: 1 }
        }
    };
    let failure_value =
        record
            .failure_code
            .as_deref()
            .map(|code| PublisherChapterWorkflowFailure {
                code: failure_code(code),
                safe_detail: record.failure_detail.clone(),
                retryable: record.state == PublisherChapterWorkflowState::RetryScheduled,
            });
    PublisherChapterWorkflowProjection {
        episode_id: record.episode_id,
        source_version: record.source_version,
        stage,
        workflow_revision: record.workflow_revision,
        attempt: record.attempt,
        max_attempts: record.max_attempts,
        request_id: record.request_id,
        cancellation_id: record.cancellation_id,
        not_before: record.not_before_ms.map(UnixTimestampMilliseconds::new),
        selected_artifact_id: record.selected_artifact_id,
        failure: failure_value,
        created_at: UnixTimestampMilliseconds::new(record.created_at_ms),
        updated_at: UnixTimestampMilliseconds::new(record.updated_at_ms),
        can_retry: matches!(
            record.state,
            PublisherChapterWorkflowState::Failed | PublisherChapterWorkflowState::Cancelled
        ),
        can_cancel: matches!(
            record.state,
            PublisherChapterWorkflowState::Requested
                | PublisherChapterWorkflowState::RetryScheduled
        ),
    }
}

fn failure_code(code: &str) -> PublisherChapterWorkflowFailureCode {
    match code {
        "offline" => PublisherChapterWorkflowFailureCode::Offline,
        "timed_out" => PublisherChapterWorkflowFailureCode::TimedOut,
        "transport" => PublisherChapterWorkflowFailureCode::Transport,
        "not_found" => PublisherChapterWorkflowFailureCode::NotFound,
        "response_too_large" => PublisherChapterWorkflowFailureCode::ResponseTooLarge,
        "invalid_response" => PublisherChapterWorkflowFailureCode::InvalidResponse,
        "invalid_document" => PublisherChapterWorkflowFailureCode::InvalidDocument,
        "selection_changed" => PublisherChapterWorkflowFailureCode::SelectionChanged,
        "storage_unavailable" => PublisherChapterWorkflowFailureCode::StorageUnavailable,
        _ => PublisherChapterWorkflowFailureCode::Unsupported { wire_code: 1 },
    }
}
