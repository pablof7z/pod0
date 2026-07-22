use pod0_application::{
    ChapterObservationProjection, ChapterObservationRejection, CoreFailureCode, HostFailureCode,
    HostObservation, OperationStage, PUBLISHER_CHAPTER_REQUEST_DEADLINE_MILLISECONDS,
    PublisherChapterObservation, publisher_chapter_retry_delay_milliseconds,
    qualify_publisher_chapter_observation,
};
use pod0_domain::ContentDigest;
use pod0_storage::{
    PublisherChapterWorkflowFailureInput, PublisherChapterWorkflowRecord,
    PublisherChapterWorkflowUpdate, StorageError,
};
use sha2::{Digest as _, Sha256};

use crate::runtime_state::{FacadeState, failure};
use crate::runtime_storage_commands::storage_failure;

const FAILURE_OFFLINE: &str = "offline";
const FAILURE_TIMED_OUT: &str = "timed_out";
const FAILURE_TRANSPORT: &str = "transport";
const FAILURE_NOT_FOUND: &str = "not_found";
const FAILURE_RESPONSE_TOO_LARGE: &str = "response_too_large";
const FAILURE_INVALID_RESPONSE: &str = "invalid_response";
const FAILURE_INVALID_DOCUMENT: &str = "invalid_document";
const FAILURE_SELECTION_CHANGED: &str = "selection_changed";

impl FacadeState {
    /// Returns `false` only when the accepted observation must be retained and
    /// replayed because a durable storage transition could not be committed.
    pub(super) fn finish_publisher_chapter_observation(
        &mut self,
        record: PublisherChapterWorkflowRecord,
        observation: HostObservation,
    ) -> bool {
        if !self.publisher_source_is_current(&record) {
            return self.restart_publisher_chapters_after_stale(record);
        }
        match observation {
            HostObservation::PublisherChaptersFetched {
                bytes,
                content_type,
                response_url,
                http_status,
                ..
            } if (200..300).contains(&http_status) => self.qualify_and_commit_publisher_chapters(
                record,
                bytes,
                content_type,
                response_url,
            ),
            HostObservation::PublisherChaptersFetched {
                http_status: 404 | 410,
                ..
            } => self.fail_publisher_chapter_attempt(record, FAILURE_NOT_FOUND, false),
            HostObservation::PublisherChaptersFetched { http_status, .. } => {
                let retryable = matches!(http_status, 408 | 425 | 429) || http_status >= 500;
                self.fail_publisher_chapter_attempt(
                    record,
                    if retryable {
                        FAILURE_TRANSPORT
                    } else {
                        FAILURE_INVALID_RESPONSE
                    },
                    retryable,
                )
            }
            HostObservation::Failed { code, .. } => {
                let (failure_code, retryable) = host_failure(code);
                self.fail_publisher_chapter_attempt(record, failure_code, retryable)
            }
            HostObservation::Cancelled => self.commit_publisher_chapter_cancellation(record),
            HostObservation::Unsupported { .. } => {
                self.fail_publisher_chapter_attempt(record, FAILURE_INVALID_RESPONSE, false)
            }
            _ => self.fail_publisher_chapter_attempt(record, FAILURE_INVALID_RESPONSE, false),
        }
    }

    fn commit_publisher_chapter_cancellation(
        &mut self,
        record: PublisherChapterWorkflowRecord,
    ) -> bool {
        let result =
            self.store
                .as_ref()
                .map_or(Err(StorageError::CutoverNotAuthoritative), |store| {
                    store.cancel_publisher_chapter_workflow(
                        record.episode_id,
                        record.workflow_revision,
                        self.now().value,
                    )
                });
        match result {
            Ok(_) => {
                self.finish(
                    record.command_id,
                    OperationStage::Cancelled,
                    Some(failure(CoreFailureCode::Cancelled)),
                    None,
                );
                true
            }
            Err(StorageError::ChapterWorkflowConflict)
            | Err(StorageError::ChapterWorkflowNotFound) => true,
            Err(_) => false,
        }
    }

    fn qualify_and_commit_publisher_chapters(
        &mut self,
        record: PublisherChapterWorkflowRecord,
        bytes: Vec<u8>,
        content_type: String,
        response_url: String,
    ) -> bool {
        let Some(episode) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == record.episode_id)
            .cloned()
        else {
            return self.restart_publisher_chapters_after_stale(record);
        };
        let digest = ContentDigest::from_bytes(Sha256::digest(&bytes).into());
        let now = self.now();
        match qualify_publisher_chapter_observation(PublisherChapterObservation {
            episode_id: episode.episode_id,
            podcast_id: episode.podcast_id,
            resolved_source_url: response_url,
            content_type,
            payload_digest: digest,
            payload: bytes,
            generated_at: now,
            duration_milliseconds: episode.duration_milliseconds,
        }) {
            ChapterObservationProjection::Qualified { artifact, .. } => {
                let result = self.store.as_ref().map_or(
                    Err(StorageError::CutoverNotAuthoritative),
                    |store| {
                        store.complete_publisher_chapter_workflow(
                            record.request_id.expect("outstanding workflow request"),
                            artifact,
                            now.value,
                        )
                    },
                );
                match result {
                    Ok(_) => {
                        match self.reload_listening() {
                            Ok(()) => self.succeed(record.command_id, None),
                            Err(error) => self.fail(record.command_id, storage_failure(error)),
                        }
                        true
                    }
                    Err(StorageError::ChapterWorkflowConflict) => {
                        self.restart_publisher_chapters_after_stale(record)
                    }
                    Err(StorageError::ChapterRevisionConflict) => self
                        .fail_publisher_chapter_attempt(record, FAILURE_SELECTION_CHANGED, false),
                    Err(StorageError::ChapterWorkflowNotFound) => true,
                    Err(_) => false,
                }
            }
            ChapterObservationProjection::Rejected { reason } => {
                let code = if reason == ChapterObservationRejection::PayloadTooLarge {
                    FAILURE_RESPONSE_TOO_LARGE
                } else {
                    FAILURE_INVALID_DOCUMENT
                };
                self.fail_publisher_chapter_attempt(record, code, false)
            }
        }
    }

    fn fail_publisher_chapter_attempt(
        &mut self,
        record: PublisherChapterWorkflowRecord,
        failure_code: &str,
        retryable: bool,
    ) -> bool {
        let now = self.now().value;
        let retry_at = retryable
            .then(|| now.checked_add(publisher_chapter_retry_delay_milliseconds(record.attempt)))
            .flatten();
        let retry_deadline = retry_at
            .and_then(|value| value.checked_add(PUBLISHER_CHAPTER_REQUEST_DEADLINE_MILLISECONDS));
        let result =
            self.store
                .as_ref()
                .map_or(Err(StorageError::CutoverNotAuthoritative), |store| {
                    store.fail_publisher_chapter_workflow(PublisherChapterWorkflowFailureInput {
                        request_id: record.request_id.expect("outstanding workflow request"),
                        failure_code: failure_code.to_owned(),
                        failure_detail: None,
                        retry_at_ms: retry_at,
                        retry_issued_revision: self.revision,
                        retry_deadline_at_ms: retry_deadline,
                        observed_at_ms: now,
                    })
                });
        match result {
            Ok(PublisherChapterWorkflowUpdate::RetryScheduled(next)) => {
                self.queue_publisher_chapter_request(next);
                self.finish(record.command_id, OperationStage::Running, None, None);
                true
            }
            Ok(PublisherChapterWorkflowUpdate::Failed(_)) => {
                self.fail(record.command_id, core_failure_for_workflow(failure_code));
                true
            }
            Err(StorageError::ChapterWorkflowConflict)
            | Err(StorageError::ChapterWorkflowNotFound) => true,
            Err(_) => false,
        }
    }

    fn publisher_source_is_current(&self, record: &PublisherChapterWorkflowRecord) -> bool {
        self.listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == record.episode_id)
            .and_then(|episode| episode.feed_metadata.chapters_url.as_deref())
            == Some(record.source_url.as_str())
    }

    fn restart_publisher_chapters_after_stale(
        &mut self,
        record: PublisherChapterWorkflowRecord,
    ) -> bool {
        self.start_publisher_chapter_workflow(
            record.episode_id,
            record.cancellation_id,
            record.command_id,
            false,
        )
    }
}

fn host_failure(code: HostFailureCode) -> (&'static str, bool) {
    match code {
        HostFailureCode::Offline => (FAILURE_OFFLINE, true),
        HostFailureCode::TimedOut => (FAILURE_TIMED_OUT, true),
        HostFailureCode::ProviderUnavailable
        | HostFailureCode::MediaUnavailable
        | HostFailureCode::IndexUnavailable
        | HostFailureCode::PlatformFailure => (FAILURE_TRANSPORT, true),
        HostFailureCode::ResponseTooLarge => (FAILURE_RESPONSE_TOO_LARGE, false),
        HostFailureCode::PermissionDenied
        | HostFailureCode::Unauthorized
        | HostFailureCode::InvalidResponse => (FAILURE_INVALID_RESPONSE, false),
        HostFailureCode::Unsupported { .. } => (FAILURE_INVALID_RESPONSE, false),
    }
}

fn core_failure_for_workflow(code: &str) -> CoreFailureCode {
    match code {
        FAILURE_OFFLINE | FAILURE_TIMED_OUT | FAILURE_TRANSPORT => CoreFailureCode::HostUnavailable,
        FAILURE_NOT_FOUND => CoreFailureCode::NotFound,
        FAILURE_SELECTION_CHANGED => CoreFailureCode::RevisionConflict,
        FAILURE_RESPONSE_TOO_LARGE | FAILURE_INVALID_RESPONSE | FAILURE_INVALID_DOCUMENT => {
            CoreFailureCode::HostRejected
        }
        _ => CoreFailureCode::InvalidChapter,
    }
}
