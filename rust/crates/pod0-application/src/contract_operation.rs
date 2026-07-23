use pod0_domain::{
    AgentTurnId, CancellationId, ClipId, ClipRevision, CommandId, ConversationId, EpisodeId,
    EvidenceGenerationId, MemoryId, MemoryRevision, NoteId, PodcastId, RecallQueryId,
    StateRevision,
};

use crate::{ChapterCommitReceipt, CoreFailure, TranscriptCommitReceipt};

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
    AgentTurnStarted {
        conversation_id: ConversationId,
        turn_id: AgentTurnId,
    },
    PublicationPrepared {
        publication_id: pod0_domain::PublicationId,
    },
    NostrSignerReady {
        account_id: pod0_domain::SignerAccountId,
    },
    NostrSignerSignedOut {
        account_id: pod0_domain::SignerAccountId,
    },
    RecallFinished {
        query_id: RecallQueryId,
        evidence_count: u16,
    },
    EvidenceRebuilt {
        episode_id: EpisodeId,
        generation_id: EvidenceGenerationId,
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
    MemoryCreated {
        memory_id: MemoryId,
        memory_revision: MemoryRevision,
        collection_revision: StateRevision,
    },
    MemoryUpdated {
        memory_id: MemoryId,
        memory_revision: MemoryRevision,
        collection_revision: StateRevision,
    },
    MemoriesCleared {
        collection_revision: StateRevision,
    },
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
