use pod0_domain::{
    CancellationId, ClipId, ClipRevision, CommandId, EpisodeId, EpisodeRecord, NoteId, PodcastId,
    PodcastRecord, PodcastSubscriptionRecord, RecallQueryId, StateRevision,
};

use crate::{
    ChapterArtifactProjection, ChapterCommitReceipt, ChapterProjectionScope, ClipProjectionScope,
    ClipsProjection, CoreFailure, EvidenceIndexProjection, MAX_OPERATION_ITEMS,
    MAX_PROJECTION_ITEMS, NoteProjectionScope, NotesProjection, PlaybackProjection,
    RecallResultProjection, TranscriptCommitReceipt, TranscriptProjection,
    TranscriptProjectionScope,
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
    Library { value: LibraryProjection },
    PodcastDetail { value: PodcastDetailProjection },
    EpisodeDetail { value: EpisodeDetailProjection },
    Playback { value: PlaybackProjection },
    Recall { value: RecallResultProjection },
    EvidenceIndex { value: EvidenceIndexProjection },
    Transcript { value: TranscriptProjection },
    Chapter { value: ChapterArtifactProjection },
    Notes { value: NotesProjection },
    Clips { value: ClipsProjection },
    Unsupported { value: UnsupportedProjection },
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

impl LibraryProjection {
    pub fn enforce_bounds(&mut self, offset: usize, requested_items: usize) {
        let item_limit = requested_items.clamp(1, usize::from(MAX_PROJECTION_ITEMS));
        let counts = (
            self.podcasts.len(),
            self.subscriptions.len(),
            self.episodes.len(),
        );
        self.podcasts = page(std::mem::take(&mut self.podcasts), offset, item_limit);
        self.subscriptions = page(std::mem::take(&mut self.subscriptions), offset, item_limit);
        self.episodes = page(std::mem::take(&mut self.episodes), offset, item_limit);
        self.operations.truncate(MAX_OPERATION_ITEMS);
        self.has_more |= counts.0 > offset.saturating_add(self.podcasts.len())
            || counts.1 > offset.saturating_add(self.subscriptions.len())
            || counts.2 > offset.saturating_add(self.episodes.len());
    }
}

fn page<T>(values: Vec<T>, offset: usize, count: usize) -> Vec<T> {
    values.into_iter().skip(offset).take(count).collect()
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PodcastDetailProjection {
    pub podcast: Option<PodcastRecord>,
    pub subscription: Option<PodcastSubscriptionRecord>,
    pub episodes: Vec<EpisodeRecord>,
    pub operations: Vec<OperationProjection>,
    pub has_more: bool,
}

impl PodcastDetailProjection {
    pub fn enforce_bounds(&mut self, offset: usize, requested_items: usize) {
        let item_limit = requested_items.clamp(1, usize::from(MAX_PROJECTION_ITEMS));
        let count = self.episodes.len();
        self.episodes = page(std::mem::take(&mut self.episodes), offset, item_limit);
        self.operations.truncate(MAX_OPERATION_ITEMS);
        self.has_more |= count > offset.saturating_add(self.episodes.len());
    }
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
