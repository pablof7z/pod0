use crate::{ActivityDomain, ApplicationCommand};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityOwner {
    Domain(ActivityDomain),
    CorrelatedEffect,
    Boundary,
}

#[must_use]
pub const fn application_command_owner(command: &ApplicationCommand) -> ActivityOwner {
    use ActivityDomain as Domain;
    use ApplicationCommand as Command;
    match command {
        Command::SubscribeToFeed { .. }
        | Command::EnsurePodcast { .. }
        | Command::RefreshPodcast { .. }
        | Command::HydratePodcastMetadata { .. }
        | Command::SearchPodcastDirectory { .. }
        | Command::LoadTopPodcasts { .. }
        | Command::ImportSharedEpisode { .. }
        | Command::SearchPodcastCatalog { .. }
        | Command::UpsertSyntheticPodcast { .. }
        | Command::UpsertExternalEpisode { .. }
        | Command::Unsubscribe { .. }
        | Command::SetSubscriptionNotifications { .. }
        | Command::SetNewEpisodeNotificationsEnabled { .. }
        | Command::SetSubscriptionAutoDownload { .. }
        | Command::SetSubscriptionTranscriptStartPolicy { .. }
        | Command::SetEpisodeStarred { .. }
        | Command::ResetListeningData => ActivityOwner::Domain(Domain::LibraryFeed),
        Command::RequestEpisodeDownload { .. }
        | Command::ReportAutomaticDownloadCandidates { .. }
        | Command::CancelEpisodeDownload { .. }
        | Command::RemoveEpisodeDownload { .. }
        | Command::ObserveDownloadEnvironment { .. } => ActivityOwner::Domain(Domain::Download),
        Command::RequestPlayback { .. } | Command::Playback { .. } => {
            ActivityOwner::Domain(Domain::Playback)
        }
        Command::RecallQuery { .. }
        | Command::ImportLegacyRecallConfiguration { .. }
        | Command::SetRecallConfiguration { .. }
        | Command::RebuildTranscriptEvidence { .. }
        | Command::CommitRecallIndexCutover => ActivityOwner::Domain(Domain::RecallKnowledge),
        Command::ImportLegacyWorkflowConfiguration { .. }
        | Command::SetWorkflowConfiguration { .. }
        | Command::ObserveWorkflowCapabilities { .. }
        | Command::ReconcileWorkflowOpportunity { .. } => ActivityOwner::Domain(Domain::Lifecycle),
        Command::CommitTranscript { .. }
        | Command::EnsureTranscriptWorkflow { .. }
        | Command::RetryTranscriptWorkflow { .. }
        | Command::CancelTranscriptWorkflow { .. } => ActivityOwner::Domain(Domain::Transcript),
        Command::EnsureScheduledTask { .. }
        | Command::UpdateScheduledTask { .. }
        | Command::RemoveScheduledTask { .. }
        | Command::ReconcileScheduledRuns
        | Command::RetryScheduledRun { .. }
        | Command::CancelScheduledRun { .. } => ActivityOwner::Domain(Domain::ScheduledAgent),
        Command::StartAgentTurn { .. }
        | Command::PublishGeneratedEpisode { .. }
        | Command::CancelAgentTurn { .. } => ActivityOwner::Domain(Domain::AgentPublication),
        Command::CommitChapter { .. }
        | Command::EnsurePublisherChapters { .. }
        | Command::RetryPublisherChapters { .. }
        | Command::CancelPublisherChapters { .. }
        | Command::EnsureModelChapters { .. }
        | Command::RetryModelChapters { .. }
        | Command::CancelModelChapters { .. } => ActivityOwner::Domain(Domain::Chapter),
        Command::CreateNote { .. }
        | Command::UpdateNote { .. }
        | Command::SetNoteDeleted { .. }
        | Command::ClearNotes { .. }
        | Command::CreateMemory { .. }
        | Command::UpdateMemory { .. }
        | Command::SetMemoryDeleted { .. }
        | Command::ClearMemories { .. }
        | Command::CreateClip { .. }
        | Command::UpdateClip { .. }
        | Command::SetClipDeleted { .. }
        | Command::ClearClips { .. } => ActivityOwner::Domain(Domain::UserArtifact),
        Command::CancelOperation { .. } => ActivityOwner::Domain(Domain::Lifecycle),
        Command::Unsupported { .. } => ActivityOwner::Boundary,
    }
}
