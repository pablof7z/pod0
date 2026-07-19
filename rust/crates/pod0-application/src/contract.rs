use pod0_domain::{
    AutoDownloadPolicy, CancellationId, CommandId, CompletionStatus, EpisodeId,
    PlaybackRatePermille, PlaybackSegment, PlaybackSleepMode, PodcastId, QueueEntry, QueueEntryId,
    StateRevision,
};

pub const FACADE_CONTRACT_VERSION: u32 = 4;
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
    UpsertExternalEpisode {
        podcast_id: PodcastId,
        feed_url: Option<String>,
        podcast_title: String,
        audio_url: String,
        title: String,
        image_url: Option<String>,
        duration_milliseconds: Option<u64>,
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
    RequestPlayback {
        episode_id: EpisodeId,
    },
    Playback {
        command: PlaybackCommand,
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
