use pod0_domain::{
    EpisodeId, EpisodeRecord, PodcastId, PodcastRecord, PodcastSubscriptionRecord, RecallQueryId,
    StateRevision,
};

use crate::{
    ChapterArtifactProjection, ChapterProjectionScope, ChapterWorkflowsProjection,
    ClipProjectionScope, ClipsProjection, DownloadWorkflowsProjection, EvidenceIndexProjection,
    MAX_PROJECTION_ITEMS, MemoriesProjection, MemoryProjectionScope, NoteProjectionScope,
    NotesProjection, OperationProjection, PlaybackProjection, RecallResultProjection,
    TranscriptProjection, TranscriptProjectionScope, TranscriptWorkflowsProjection,
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
    TranscriptWorkflows {
        episode_id: Option<EpisodeId>,
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
    ScheduledAgent {
        task_id: Option<pod0_domain::ScheduledTaskId>,
    },
    AgentConversations,
    AgentConversation {
        conversation_id: pod0_domain::ConversationId,
    },
    Publications {
        publication_id: Option<pod0_domain::PublicationId>,
    },
    NostrSigner,
    Notes {
        scope: NoteProjectionScope,
    },
    Memories {
        scope: MemoryProjectionScope,
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
    TranscriptWorkflows {
        value: TranscriptWorkflowsProjection,
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
    ScheduledAgent {
        value: crate::ScheduledAgentProjection,
    },
    AgentConversations {
        value: crate::AgentConversationsProjection,
    },
    AgentConversation {
        value: crate::AgentConversationProjection,
    },
    Publications {
        value: crate::PublicationsProjection,
    },
    NostrSigner {
        value: crate::SignerProjection,
    },
    Notes {
        value: NotesProjection,
    },
    Memories {
        value: MemoriesProjection,
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
