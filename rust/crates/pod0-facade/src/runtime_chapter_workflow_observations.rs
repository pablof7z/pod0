use pod0_application::{CoreFailureCode, HostFailureCode};
use pod0_storage::PublisherChapterWorkflowRecord;

use crate::runtime_state::FacadeState;

pub(super) const FAILURE_OFFLINE: &str = "offline";
pub(super) const FAILURE_TIMED_OUT: &str = "timed_out";
pub(super) const FAILURE_TRANSPORT: &str = "transport";
pub(super) const FAILURE_NOT_FOUND: &str = "not_found";
pub(super) const FAILURE_RESPONSE_TOO_LARGE: &str = "response_too_large";
pub(super) const FAILURE_INVALID_RESPONSE: &str = "invalid_response";
pub(super) const FAILURE_INVALID_DOCUMENT: &str = "invalid_document";
pub(super) const FAILURE_SELECTION_CHANGED: &str = "selection_changed";

impl FacadeState {
    pub(super) fn publisher_source_is_current(
        &self,
        record: &PublisherChapterWorkflowRecord,
    ) -> bool {
        self.listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == record.episode_id)
            .and_then(|episode| episode.feed_metadata.chapters_url.as_deref())
            == Some(record.source_url.as_str())
    }
}

pub(super) fn host_failure(code: HostFailureCode) -> (&'static str, bool) {
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

pub(super) fn core_failure_for_workflow(code: &str) -> CoreFailureCode {
    match code {
        FAILURE_OFFLINE | FAILURE_TIMED_OUT | FAILURE_TRANSPORT => CoreFailureCode::HostUnavailable,
        FAILURE_NOT_FOUND => CoreFailureCode::NotFound,
        FAILURE_SELECTION_CHANGED => CoreFailureCode::RevisionConflict,
        FAILURE_RESPONSE_TOO_LARGE | FAILURE_INVALID_RESPONSE | FAILURE_INVALID_DOCUMENT => {
            CoreFailureCode::HostRejected
        }
        _ => CoreFailureCode::StorageUnavailable,
    }
}
