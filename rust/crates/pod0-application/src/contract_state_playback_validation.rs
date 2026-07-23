pub(super) fn playback_request_episode_id(
    request: &crate::HostRequest,
) -> Option<pod0_domain::EpisodeId> {
    use crate::HostRequest as Request;

    match request {
        Request::LoadMedia { episode_id, .. }
        | Request::Play { episode_id, .. }
        | Request::Pause { episode_id }
        | Request::Seek { episode_id, .. }
        | Request::SetRate { episode_id, .. }
        | Request::ArmNativeTimer { episode_id, .. }
        | Request::CancelNativeTimer { episode_id }
        | Request::StopPlayback { episode_id } => Some(*episode_id),
        Request::FetchFeed { .. }
        | Request::ObservePlayback { .. }
        | Request::EmbedRecallQuery { .. }
        | Request::EmbedRecallSpans { .. }
        | Request::RerankRecallCandidates { .. }
        | Request::FetchPublisherChapters { .. }
        | Request::ExecuteChapterModel { .. }
        | Request::RecoverChapterModelOperation { .. }
        | Request::StartEpisodeDownload { .. }
        | Request::CancelEpisodeDownload { .. }
        | Request::RemoveEpisodeDownloadArtifact { .. }
        | Request::ExecuteTranscriptCapability { .. }
        | Request::ExecuteScheduledAgentTurn { .. }
        | Request::ExecuteAgentModelTurn { .. }
        | Request::PresentAgentApproval { .. }
        | Request::ExecuteAgentCapability { .. }
        | Request::ScheduleCoreWake { .. }
        | Request::RemoveLegacyRecallIndexArtifacts
        | Request::ProvisionNostrSignerCredential
        | Request::RestoreNostrSignerCredential { .. }
        | Request::SignNostrEvent { .. }
        | Request::DeleteNostrSignerCredential { .. }
        | Request::Unsupported { .. } => None,
    }
}
