use pod0_application::{
    ChapterWorkflowsProjection, CoreFailureCode, PublisherChapterWorkflowFailure,
    PublisherChapterWorkflowFailureCode, PublisherChapterWorkflowProjection,
    PublisherChapterWorkflowStage,
};
use pod0_domain::{EpisodeId, UnixTimestampMilliseconds};
use pod0_storage::{PublisherChapterWorkflowRecord, PublisherChapterWorkflowState};

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
                has_more: false,
                failure: Some(failure(CoreFailureCode::StorageUnavailable)),
            };
        };
        let page = match store.publisher_chapter_workflow_page(
            episode_id,
            u32::try_from(offset).unwrap_or(u32::MAX),
            u16::try_from(item_limit).unwrap_or(pod0_application::MAX_PROJECTION_ITEMS),
        ) {
            Ok(page) => page,
            Err(_) => {
                return ChapterWorkflowsProjection {
                    publisher: Vec::new(),
                    has_more: false,
                    failure: Some(failure(CoreFailureCode::StorageUnavailable)),
                };
            }
        };
        ChapterWorkflowsProjection {
            publisher: page.items.into_iter().map(publisher_projection).collect(),
            has_more: page.has_more,
            failure: None,
        }
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
