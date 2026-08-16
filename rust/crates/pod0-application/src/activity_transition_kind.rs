#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LibraryFeedTransition {
    SubscriptionChanged,
    FeedFetchStateChanged,
    FeedDiscoveryStateChanged,
    EpisodeMetadataChanged,
    EpisodeStarredChanged,
    NotificationPreferenceChanged,
    NotificationDeliveryStateChanged,
    TranscriptPreferenceChanged,
    ListeningDataReset,
    ListeningAuthorityChanged,
    FeedDiscoveryAuthorityChanged,
    LibraryNetworkStateChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlaybackTransition {
    SessionStateChanged,
    QueueChanged,
    PositionCheckpointCommitted,
    RateChanged,
    SleepTimerChanged,
    InterruptionHandled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DownloadTransition {
    DesiredStateChanged,
    EnvironmentChanged,
    AttemptStateChanged,
    ArtifactAdopted,
    ArtifactRemoved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TranscriptTransition {
    WorkflowStateChanged,
    AttemptStateChanged,
    ArtifactAdopted,
    SelectionChanged,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChapterTransition {
    PublisherWorkflowStateChanged,
    ModelWorkflowStateChanged,
    ArtifactAdopted,
    SelectionChanged,
    PlaybackChapterChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RecallKnowledgeTransition {
    ConfigurationChanged,
    EvidenceGenerationChanged,
    IndexCutoverChanged,
    QueryStateChanged,
    RankingCompleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ScheduledAgentActivityTransition {
    TaskChanged,
    OccurrenceStateChanged,
    AttemptStateChanged,
    ArtifactAdopted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AgentPublicationTransition {
    TurnStateChanged,
    ApprovalChanged,
    ToolStateChanged,
    ArtifactAdopted,
    PublicationStateChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UserArtifactTransition {
    NoteChanged,
    MemoryChanged,
    ClipChanged,
    CategoryChanged,
    SpeakerIdentityChanged,
    SpeakerAssignmentChanged,
    SettingChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LifecycleTransition {
    CancellationChanged,
    WakeReached,
    RecoveryChanged,
    AuthorityCutoverChanged,
    UserDataErasureChanged,
    WorkflowConfigurationChanged,
    WorkflowCapabilitiesObserved,
    WorkflowReconciliationPlanned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DomainTransitionKind {
    LibraryFeed(LibraryFeedTransition),
    Playback(PlaybackTransition),
    Download(DownloadTransition),
    Transcript(TranscriptTransition),
    Chapter(ChapterTransition),
    RecallKnowledge(RecallKnowledgeTransition),
    ScheduledAgent(ScheduledAgentActivityTransition),
    AgentPublication(AgentPublicationTransition),
    UserArtifact(UserArtifactTransition),
    Lifecycle(LifecycleTransition),
}
