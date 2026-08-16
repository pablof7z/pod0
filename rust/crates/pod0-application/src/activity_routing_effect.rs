use crate::{ActivityDomain, ActivityOwner, HostRequest};

#[must_use]
pub const fn host_request_owner(request: &HostRequest) -> ActivityOwner {
    use ActivityDomain as Domain;
    use HostRequest as Request;
    match request {
        Request::CancelAuthorizedEffect { .. } => ActivityOwner::CorrelatedEffect,
        Request::FetchFeed { .. } | Request::DeliverNewEpisodeNotification { .. } => {
            ActivityOwner::Domain(Domain::LibraryFeed)
        }
        Request::FetchLibraryDocument { .. } => ActivityOwner::Domain(Domain::LibraryFeed),
        Request::LoadMedia { .. }
        | Request::Play { .. }
        | Request::Pause { .. }
        | Request::Seek { .. }
        | Request::SetRate { .. }
        | Request::ArmNativeTimer { .. }
        | Request::CancelNativeTimer { .. }
        | Request::ObservePlayback { .. }
        | Request::StopPlayback { .. } => ActivityOwner::Domain(Domain::Playback),
        Request::EmbedRecallQuery { .. }
        | Request::EmbedRecallSpans { .. }
        | Request::RerankRecallCandidates { .. }
        | Request::RemoveLegacyRecallIndexArtifacts => {
            ActivityOwner::Domain(Domain::RecallKnowledge)
        }
        Request::FetchPublisherChapters { .. }
        | Request::ExecuteChapterModel { .. }
        | Request::RecoverChapterModelOperation { .. } => ActivityOwner::Domain(Domain::Chapter),
        Request::StartEpisodeDownload { .. }
        | Request::CancelEpisodeDownload { .. }
        | Request::RemoveEpisodeDownloadArtifact { .. } => ActivityOwner::Domain(Domain::Download),
        Request::ExecuteTranscriptCapability { .. } => ActivityOwner::Domain(Domain::Transcript),
        Request::ExecuteScheduledAgentTurn { .. } => ActivityOwner::Domain(Domain::ScheduledAgent),
        Request::ExecuteAgentModelTurn { .. }
        | Request::PresentAgentApproval { .. }
        | Request::ExecuteAgentCapability { .. } => ActivityOwner::Domain(Domain::AgentPublication),
        Request::ScheduleCoreWake { .. } => ActivityOwner::Domain(Domain::Lifecycle),
        Request::Unsupported { .. } => ActivityOwner::Boundary,
    }
}
