#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum UserDataTargetKind {
    CoreSqlite,
    CoreWal,
    CoreShm,
    EpisodeSqlite,
    EpisodeWal,
    EpisodeShm,
    RecallIndex,
    RecallIndexWal,
    RecallIndexShm,
    DownloadedMediaRoot,
    StagedMediaRoot,
    TranscriptArtifactRoot,
    LegacyTranscriptRoot,
    ChapterArtifactRoot,
    MigrationBackupRoot,
    ApplicationStateProjection,
    NativeObservationOutbox,
    NativeObservationLease,
    AgentGeneratedAudioRoot,
    LegacyChatHistoryRoot,
    LegacyWorkflowStore,
    LegacyWorkflowArtifactRoot,
    CostLedger,
    AgentConversationPointer,
    SpotlightIndex,
    NowPlayingProjection,
    ProductSignals,
}

impl UserDataTargetKind {
    pub const ALL: [Self; 27] = [
        Self::CoreSqlite,
        Self::CoreWal,
        Self::CoreShm,
        Self::EpisodeSqlite,
        Self::EpisodeWal,
        Self::EpisodeShm,
        Self::RecallIndex,
        Self::RecallIndexWal,
        Self::RecallIndexShm,
        Self::DownloadedMediaRoot,
        Self::StagedMediaRoot,
        Self::TranscriptArtifactRoot,
        Self::LegacyTranscriptRoot,
        Self::ChapterArtifactRoot,
        Self::MigrationBackupRoot,
        Self::ApplicationStateProjection,
        Self::NativeObservationOutbox,
        Self::NativeObservationLease,
        Self::AgentGeneratedAudioRoot,
        Self::LegacyChatHistoryRoot,
        Self::LegacyWorkflowStore,
        Self::LegacyWorkflowArtifactRoot,
        Self::CostLedger,
        Self::AgentConversationPointer,
        Self::SpotlightIndex,
        Self::NowPlayingProjection,
        Self::ProductSignals,
    ];

    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::CoreSqlite => "core_sqlite",
            Self::CoreWal => "core_wal",
            Self::CoreShm => "core_shm",
            Self::EpisodeSqlite => "episode_sqlite",
            Self::EpisodeWal => "episode_wal",
            Self::EpisodeShm => "episode_shm",
            Self::RecallIndex => "recall_index",
            Self::RecallIndexWal => "recall_index_wal",
            Self::RecallIndexShm => "recall_index_shm",
            Self::DownloadedMediaRoot => "downloaded_media_root",
            Self::StagedMediaRoot => "staged_media_root",
            Self::TranscriptArtifactRoot => "transcript_artifact_root",
            Self::LegacyTranscriptRoot => "legacy_transcript_root",
            Self::ChapterArtifactRoot => "chapter_artifact_root",
            Self::MigrationBackupRoot => "migration_backup_root",
            Self::ApplicationStateProjection => "application_state_projection",
            Self::NativeObservationOutbox => "native_observation_outbox",
            Self::NativeObservationLease => "native_observation_lease",
            Self::AgentGeneratedAudioRoot => "agent_generated_audio_root",
            Self::LegacyChatHistoryRoot => "legacy_chat_history_root",
            Self::LegacyWorkflowStore => "legacy_workflow_store",
            Self::LegacyWorkflowArtifactRoot => "legacy_workflow_artifact_root",
            Self::CostLedger => "cost_ledger",
            Self::AgentConversationPointer => "agent_conversation_pointer",
            Self::SpotlightIndex => "spotlight_index",
            Self::NowPlayingProjection => "now_playing_projection",
            Self::ProductSignals => "product_signals",
        }
    }

    pub const fn native_action_identifier(self) -> Option<&'static str> {
        match self {
            Self::AgentConversationPointer => Some("pod0.agent.lastConversationID.v1"),
            Self::SpotlightIndex => Some("pod0.search.notes,memories,subscriptions,episodes"),
            Self::NowPlayingProjection => Some("group.com.podcastr.app/now-playing-snapshot.v1"),
            _ => None,
        }
    }

    pub(super) const fn allows_multiple_targets(self) -> bool {
        matches!(self, Self::MigrationBackupRoot)
    }

    pub const fn covering_kind(self) -> Option<Self> {
        match self {
            Self::TranscriptArtifactRoot | Self::ChapterArtifactRoot => Some(Self::CoreSqlite),
            Self::LegacyWorkflowStore => Some(Self::EpisodeSqlite),
            _ => None,
        }
    }
}
