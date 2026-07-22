use pod0_domain::{
    CompletionStatus, EpisodeId, PlaybackRatePermille, PlaybackSegment, PlaybackSleepMode,
    QueueEntry, QueueEntryId,
};

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
    NextChapter {
        context: crate::ChapterPlaybackContext,
        position_milliseconds: u64,
    },
    PreviousChapter {
        context: crate::ChapterPlaybackContext,
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
        auto_skip_ads: bool,
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
