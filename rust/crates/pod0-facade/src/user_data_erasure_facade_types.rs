use std::sync::Arc;

use pod0_domain::CommandId;
use pod0_storage::UserDataErasureConfirmation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ErasureLifecycle {
    Active,
    Prepared,
    Erasing,
    RecoveryRequired,
    Erased,
}

#[derive(Debug, uniffi::Object)]
pub struct UserDataErasureToken {
    pub(super) operation_id: CommandId,
}

pub(super) struct PreparedFacadeErasure {
    pub token: Arc<UserDataErasureToken>,
    pub confirmation: UserDataErasureConfirmation,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UserDataErasureTargetLocation {
    pub kind: UserDataErasureTargetKind,
    pub location: String,
    pub covered_by: Option<UserDataErasureTargetKind>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UserDataErasureLocations {
    pub recovery_root: String,
    pub allowed_roots: Vec<String>,
    pub targets: Vec<UserDataErasureTargetLocation>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct NativeErasureAction {
    pub action_id: CommandId,
    pub operation_id: CommandId,
    pub kind: UserDataErasureTargetKind,
    pub identifier: String,
    pub attempt: u16,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum UserDataErasureResult {
    AwaitingNativeActions { actions: Vec<NativeErasureAction> },
    Complete { fresh_store_id: CommandId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum UserDataErasureTargetKind {
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

impl UserDataErasureTargetKind {
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
}

#[derive(Debug, uniffi::Error)]
pub enum UserDataErasureError {
    Conflict,
    RecoveryRequired,
}

impl std::fmt::Display for UserDataErasureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Conflict => "user-data erasure confirmation conflict",
            Self::RecoveryRequired => "user-data erasure requires forward recovery",
        })
    }
}

impl std::error::Error for UserDataErasureError {}
