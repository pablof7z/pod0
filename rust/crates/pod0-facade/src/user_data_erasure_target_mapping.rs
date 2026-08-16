use pod0_storage::UserDataTargetKind;

use crate::UserDataErasureTargetKind;

impl From<UserDataErasureTargetKind> for UserDataTargetKind {
    fn from(value: UserDataErasureTargetKind) -> Self {
        use UserDataErasureTargetKind as F;
        match value {
            F::CoreSqlite => Self::CoreSqlite,
            F::CoreWal => Self::CoreWal,
            F::CoreShm => Self::CoreShm,
            F::EpisodeSqlite => Self::EpisodeSqlite,
            F::EpisodeWal => Self::EpisodeWal,
            F::EpisodeShm => Self::EpisodeShm,
            F::RecallIndex => Self::RecallIndex,
            F::RecallIndexWal => Self::RecallIndexWal,
            F::RecallIndexShm => Self::RecallIndexShm,
            F::DownloadedMediaRoot => Self::DownloadedMediaRoot,
            F::StagedMediaRoot => Self::StagedMediaRoot,
            F::TranscriptArtifactRoot => Self::TranscriptArtifactRoot,
            F::LegacyTranscriptRoot => Self::LegacyTranscriptRoot,
            F::ChapterArtifactRoot => Self::ChapterArtifactRoot,
            F::MigrationBackupRoot => Self::MigrationBackupRoot,
            F::ApplicationStateProjection => Self::ApplicationStateProjection,
            F::NativeObservationOutbox => Self::NativeObservationOutbox,
            F::NativeObservationLease => Self::NativeObservationLease,
            F::AgentGeneratedAudioRoot => Self::AgentGeneratedAudioRoot,
            F::LegacyChatHistoryRoot => Self::LegacyChatHistoryRoot,
            F::LegacyWorkflowStore => Self::LegacyWorkflowStore,
            F::LegacyWorkflowArtifactRoot => Self::LegacyWorkflowArtifactRoot,
            F::CostLedger => Self::CostLedger,
            F::AgentConversationPointer => Self::AgentConversationPointer,
            F::SpotlightIndex => Self::SpotlightIndex,
            F::NowPlayingProjection => Self::NowPlayingProjection,
            F::ProductSignals => Self::ProductSignals,
        }
    }
}

impl From<UserDataTargetKind> for UserDataErasureTargetKind {
    fn from(value: UserDataTargetKind) -> Self {
        use UserDataTargetKind as S;
        match value {
            S::CoreSqlite => Self::CoreSqlite,
            S::CoreWal => Self::CoreWal,
            S::CoreShm => Self::CoreShm,
            S::EpisodeSqlite => Self::EpisodeSqlite,
            S::EpisodeWal => Self::EpisodeWal,
            S::EpisodeShm => Self::EpisodeShm,
            S::RecallIndex => Self::RecallIndex,
            S::RecallIndexWal => Self::RecallIndexWal,
            S::RecallIndexShm => Self::RecallIndexShm,
            S::DownloadedMediaRoot => Self::DownloadedMediaRoot,
            S::StagedMediaRoot => Self::StagedMediaRoot,
            S::TranscriptArtifactRoot => Self::TranscriptArtifactRoot,
            S::LegacyTranscriptRoot => Self::LegacyTranscriptRoot,
            S::ChapterArtifactRoot => Self::ChapterArtifactRoot,
            S::MigrationBackupRoot => Self::MigrationBackupRoot,
            S::ApplicationStateProjection => Self::ApplicationStateProjection,
            S::NativeObservationOutbox => Self::NativeObservationOutbox,
            S::NativeObservationLease => Self::NativeObservationLease,
            S::AgentGeneratedAudioRoot => Self::AgentGeneratedAudioRoot,
            S::LegacyChatHistoryRoot => Self::LegacyChatHistoryRoot,
            S::LegacyWorkflowStore => Self::LegacyWorkflowStore,
            S::LegacyWorkflowArtifactRoot => Self::LegacyWorkflowArtifactRoot,
            S::CostLedger => Self::CostLedger,
            S::AgentConversationPointer => Self::AgentConversationPointer,
            S::SpotlightIndex => Self::SpotlightIndex,
            S::NowPlayingProjection => Self::NowPlayingProjection,
            S::ProductSignals => Self::ProductSignals,
        }
    }
}
