use pod0_domain::HostRequestId;

use super::HostRequestLedger;
use crate::HostRequest;

impl HostRequestLedger {
    #[must_use]
    pub fn is_playback_observation_stream(&self, request_id: HostRequestId) -> bool {
        self.requests.get(&request_id).is_some_and(|request| {
            matches!(
                request.envelope.request,
                HostRequest::ObservePlayback { .. }
            )
        })
    }

    #[must_use]
    pub fn is_chapter_model_request(&self, request_id: HostRequestId) -> bool {
        self.requests.get(&request_id).is_some_and(|request| {
            matches!(
                request.envelope.request,
                HostRequest::ExecuteChapterModel { .. }
                    | HostRequest::RecoverChapterModelOperation { .. }
            )
        })
    }

    #[must_use]
    pub fn is_publisher_chapter_request(&self, request_id: HostRequestId) -> bool {
        self.requests.get(&request_id).is_some_and(|request| {
            matches!(
                request.envelope.request,
                HostRequest::FetchPublisherChapters { .. }
            )
        })
    }

    #[must_use]
    pub fn is_download_request(&self, request_id: HostRequestId) -> bool {
        self.requests.get(&request_id).is_some_and(|request| {
            matches!(
                request.envelope.request,
                HostRequest::StartEpisodeDownload { .. }
                    | HostRequest::CancelEpisodeDownload { .. }
                    | HostRequest::RemoveEpisodeDownloadArtifact { .. }
            )
        })
    }

    #[must_use]
    pub fn is_transcript_request(&self, request_id: HostRequestId) -> bool {
        self.requests.get(&request_id).is_some_and(|request| {
            matches!(
                request.envelope.request,
                HostRequest::ExecuteTranscriptCapability { .. }
            )
        })
    }

    #[must_use]
    pub fn is_core_wake_request(&self, request_id: HostRequestId) -> bool {
        self.requests.get(&request_id).is_some_and(|request| {
            matches!(
                request.envelope.request,
                HostRequest::ScheduleCoreWake { .. }
            )
        })
    }

    #[must_use]
    pub fn is_playback_request(&self, request_id: HostRequestId) -> bool {
        self.requests.get(&request_id).is_some_and(|request| {
            matches!(
                request.envelope.request,
                HostRequest::LoadMedia { .. }
                    | HostRequest::Play { .. }
                    | HostRequest::Pause { .. }
                    | HostRequest::Seek { .. }
                    | HostRequest::SetRate { .. }
                    | HostRequest::ArmNativeTimer { .. }
                    | HostRequest::CancelNativeTimer { .. }
                    | HostRequest::ObservePlayback { .. }
                    | HostRequest::StopPlayback { .. }
            )
        })
    }
}
