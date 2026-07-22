use pod0_domain::{
    CancellationId, ClipId, ClipRevision, CommandId, EpisodeId, EpisodeRecord, NoteId, PodcastId,
    PodcastRecord, PodcastSubscriptionRecord, RecallQueryId, StateRevision,
};

use crate::{
    ChapterArtifactProjection, ChapterCommitReceipt, ChapterProjectionScope,
    ChapterWorkflowsProjection, ClipProjectionScope, ClipsProjection, CoreFailure,
    DownloadWorkflowsProjection, EvidenceIndexProjection, MAX_PROJECTION_ITEMS,
    NoteProjectionScope, NotesProjection, PlaybackProjection, RecallResultProjection,
    TranscriptCommitReceipt, TranscriptProjection, TranscriptProjectionScope,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ProjectionScope {
    Library,
    PodcastDetail {
        podcast_id: PodcastId,
    },
    EpisodeDetail {
        episode_id: EpisodeId,
    },
    Playback,
    RecallConfiguration,
    Recall {
        query_id: RecallQueryId,
    },
    EvidenceIndex {
        episode_id: EpisodeId,
    },
    Transcript {
        episode_id: EpisodeId,
        scope: TranscriptProjectionScope,
    },
    Chapter {
        episode_id: EpisodeId,
        scope: ChapterProjectionScope,
    },
    ChapterWorkflows {
        episode_id: Option<EpisodeId>,
    },
    Downloads {
        episode_id: Option<EpisodeId>,
    },
    Notes {
        scope: NoteProjectionScope,
    },
    Clips {
        scope: ClipProjectionScope,
    },
    Unsupported {
        wire_code: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ProjectionRequest {
    pub scope: ProjectionScope,
    pub offset: u32,
    pub max_items: u16,
}

impl ProjectionRequest {
    #[must_use]
    pub fn bounded_max_items(self) -> usize {
        usize::from(self.max_items.clamp(1, MAX_PROJECTION_ITEMS))
    }

    #[must_use]
    pub fn bounded_offset(self) -> usize {
        usize::try_from(self.offset).unwrap_or(usize::MAX)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ProjectionEnvelope {
    pub contract_version: u32,
    pub state_revision: StateRevision,
    pub projection: Projection,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
// UniFFI enum payloads must remain value records so Swift and Kotlin receive
// the same generated shape. Every collection is bounded before serialization;
// boxing only one Rust variant would add indirection without reducing FFI work.
#[allow(clippy::large_enum_variant)]
pub enum Projection {
    Library {
        value: LibraryProjection,
    },
    PodcastDetail {
        value: PodcastDetailProjection,
    },
    EpisodeDetail {
        value: EpisodeDetailProjection,
    },
    Playback {
        value: PlaybackProjection,
    },
    RecallConfiguration {
        value: pod0_domain::RecallConfiguration,
    },
    Recall {
        value: RecallResultProjection,
    },
    EvidenceIndex {
        value: EvidenceIndexProjection,
    },
    Transcript {
        value: TranscriptProjection,
    },
    Chapter {
        value: ChapterArtifactProjection,
    },
    ChapterWorkflows {
        value: ChapterWorkflowsProjection,
    },
    Downloads {
        value: DownloadWorkflowsProjection,
    },
    Notes {
        value: NotesProjection,
    },
    Clips {
        value: ClipsProjection,
    },
    Unsupported {
        value: UnsupportedProjection,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct UnsupportedProjection {
    pub wire_code: u32,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LibraryProjection {
    pub podcasts: Vec<PodcastRecord>,
    pub subscriptions: Vec<PodcastSubscriptionRecord>,
    pub episodes: Vec<EpisodeRecord>,
    pub operations: Vec<OperationProjection>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PodcastDetailProjection {
    pub podcast: Option<PodcastRecord>,
    pub subscription: Option<PodcastSubscriptionRecord>,
    pub episodes: Vec<EpisodeRecord>,
    pub operations: Vec<OperationProjection>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EpisodeDetailProjection {
    pub episode: Option<EpisodeRecord>,
    pub podcast: Option<PodcastRecord>,
    pub subscription: Option<PodcastSubscriptionRecord>,
    pub operations: Vec<OperationProjection>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PodcastSummary {
    pub podcast_id: PodcastId,
    pub title: String,
    pub subscribed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EpisodeSummary {
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub title: String,
    pub duration_milliseconds: Option<u64>,
    pub resume_position_milliseconds: u64,
    pub completed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct OperationProjection {
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub stage: OperationStage,
    pub failure: Option<CoreFailure>,
    pub result: Option<OperationResult>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum OperationResult {
    Podcast {
        podcast_id: PodcastId,
    },
    ExternalEpisode {
        podcast_id: PodcastId,
        episode_id: EpisodeId,
    },
    RemovedPodcast {
        podcast_id: PodcastId,
    },
    PreferencesUpdated {
        podcast_id: PodcastId,
    },
    EpisodeUpdated {
        episode_id: EpisodeId,
    },
    ListeningReset,
    PlaybackUpdated {
        episode_id: Option<EpisodeId>,
    },
    QueueUpdated,
    RecallFinished {
        query_id: RecallQueryId,
        evidence_count: u16,
    },
    EvidenceRebuilt {
        episode_id: EpisodeId,
        generation_id: pod0_domain::EvidenceGenerationId,
        span_count: u32,
    },
    RecallIndexCutoverCommitted {
        schema_version: u32,
        removed_legacy_file_count: u8,
    },
    RecallConfigurationImported {
        imported: bool,
        revision: StateRevision,
    },
    RecallConfigurationUpdated {
        revision: StateRevision,
        reindexed_episode_count: u32,
    },
    TranscriptCommitted {
        receipt: TranscriptCommitReceipt,
    },
    ChapterCommitted {
        receipt: ChapterCommitReceipt,
    },
    NoteCreated {
        note_id: NoteId,
    },
    NoteUpdated {
        note_id: NoteId,
    },
    NotesCleared,
    ClipCreated {
        clip_id: ClipId,
        clip_revision: ClipRevision,
        collection_revision: StateRevision,
    },
    ClipUpdated {
        clip_id: ClipId,
        clip_revision: ClipRevision,
        collection_revision: StateRevision,
    },
    ClipsCleared {
        collection_revision: StateRevision,
    },
    Unsupported {
        wire_code: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum OperationStage {
    Accepted,
    Running,
    Blocked,
    Failed,
    Cancelled,
    Succeeded,
    Unsupported { wire_code: u32 },
}

impl OperationStage {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Failed | Self::Cancelled | Self::Succeeded | Self::Unsupported { .. }
        )
    }
}
