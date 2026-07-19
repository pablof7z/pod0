use pod0_domain::{
    AutoDownloadPolicy, CancellationId, CommandId, CompletionStatus, EpisodeId, NoteAuthor, NoteId,
    NoteKind, NoteRevision, NoteTarget, PlaybackRatePermille, PlaybackSegment, PlaybackSleepMode,
    PodcastId, QueueEntry, QueueEntryId, StateRevision, UnixTimestampMilliseconds,
};

use crate::{EvidenceChunkPolicy, RecallQuery, TranscriptEvidenceInput};

pub const FACADE_CONTRACT_VERSION: u32 = 9;
pub const MAX_PROJECTION_ITEMS: u16 = 200;
pub const MAX_OPERATION_ITEMS: usize = 32;
pub const MAX_HOST_REQUEST_BATCH: u16 = 64;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CommandEnvelope {
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub expected_revision: Option<StateRevision>,
    pub command: ApplicationCommand,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SyntheticPodcastInput {
    /// `None` creates a new stable ID derived from the command identity.
    /// Updates and named built-ins provide their existing stable ID.
    pub podcast_id: Option<PodcastId>,
    pub title: String,
    pub author: String,
    pub image_url: Option<String>,
    pub description: String,
    pub language: Option<String>,
    pub categories: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ExternalEpisodeInput {
    pub podcast_id: PodcastId,
    pub feed_url: Option<String>,
    pub podcast_title: String,
    pub audio_url: String,
    pub title: String,
    pub description: String,
    pub published_at: UnixTimestampMilliseconds,
    pub enclosure_mime_type: Option<String>,
    pub image_url: Option<String>,
    pub duration_milliseconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ApplicationCommand {
    SubscribeToFeed {
        feed_url: String,
    },
    EnsurePodcast {
        feed_url: String,
    },
    RefreshPodcast {
        podcast_id: PodcastId,
    },
    HydratePodcastMetadata {
        podcast_id: PodcastId,
    },
    UpsertSyntheticPodcast {
        podcast: SyntheticPodcastInput,
    },
    UpsertExternalEpisode {
        episode: ExternalEpisodeInput,
    },
    Unsubscribe {
        podcast_id: PodcastId,
    },
    SetSubscriptionNotifications {
        podcast_id: PodcastId,
        enabled: bool,
    },
    SetSubscriptionAutoDownload {
        podcast_id: PodcastId,
        policy: AutoDownloadPolicy,
    },
    SetEpisodeStarred {
        episode_id: EpisodeId,
        starred: bool,
    },
    ResetListeningData,
    RequestPlayback {
        episode_id: EpisodeId,
    },
    Playback {
        command: PlaybackCommand,
    },
    RecallQuery {
        query: RecallQuery,
    },
    RebuildTranscriptEvidence {
        input: TranscriptEvidenceInput,
        policy: EvidenceChunkPolicy,
    },
    CreateNote {
        text: String,
        kind: NoteKind,
        author: NoteAuthor,
        target: Option<NoteTarget>,
    },
    UpdateNote {
        note_id: NoteId,
        expected_note_revision: NoteRevision,
        text: String,
        kind: NoteKind,
        target: Option<NoteTarget>,
    },
    SetNoteDeleted {
        note_id: NoteId,
        expected_note_revision: NoteRevision,
        deleted: bool,
    },
    ClearNotes {
        expected_collection_revision: StateRevision,
    },
    CancelOperation {
        cancellation_id: CancellationId,
    },
    Unsupported {
        wire_code: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PlaybackCommand {
    Select {
        episode_id: EpisodeId,
        segment: Option<PlaybackSegment>,
        label: Option<String>,
    },
    Restore,
    Play,
    Pause,
    Seek {
        position_milliseconds: u64,
    },
    Enqueue {
        entry: QueueEntry,
        placement: QueuePlacement,
    },
    RemoveQueueEntry {
        queue_entry_id: QueueEntryId,
    },
    RemoveEpisodeFromQueue {
        episode_id: EpisodeId,
    },
    ReplaceQueueOrder {
        queue_entry_ids: Vec<QueueEntryId>,
    },
    ClearQueue,
    AdvanceQueue,
    SetRate {
        rate: PlaybackRatePermille,
    },
    SetSleepTimer {
        mode: PlaybackSleepMode,
    },
    SetPreferences {
        auto_mark_played_at_natural_end: bool,
        auto_play_next: bool,
    },
    SetCompletion {
        episode_id: EpisodeId,
        completion: CompletionStatus,
    },
    ResetProgress {
        episode_id: EpisodeId,
    },
    Checkpoint {
        episode_id: EpisodeId,
        position_milliseconds: u64,
    },
    NativeTimerFired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum QueuePlacement {
    Back,
    Next,
    Unsupported { wire_code: u32 },
}
