use pod0_domain::{
    ChapterArtifactId, ChapterPlaybackSessionId, EpisodeId, PlaybackRatePermille, PlaybackSegment,
    PlaybackSleepMode, QueueEntry, StateRevision,
};

use crate::{MAX_OPERATION_ITEMS, MAX_PROJECTION_ITEMS, OperationProjection, PlaybackHostState};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PlaybackProjection {
    pub current: Option<PlaybackItem>,
    pub queue: Vec<QueueEntry>,
    pub rate: PlaybackRatePermille,
    pub sleep_mode: PlaybackSleepMode,
    pub auto_mark_played_at_natural_end: bool,
    pub auto_play_next: bool,
    pub auto_skip_ads: bool,
    pub allowed_actions: PlaybackAllowedActions,
    pub host_state: PlaybackHostState,
    pub operations: Vec<OperationProjection>,
}

impl PlaybackProjection {
    pub fn enforce_bounds(&mut self, requested_items: usize) {
        self.queue
            .truncate(requested_items.clamp(1, usize::from(MAX_PROJECTION_ITEMS)));
        self.operations.truncate(MAX_OPERATION_ITEMS);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PlaybackItem {
    pub episode_id: EpisodeId,
    pub title: String,
    pub durable_resume_position_milliseconds: u64,
    /// A committed core fact used by native product-validation adapters.
    /// Native code records the typed outcome but does not choose the threshold.
    pub meaningful_listening_reached: bool,
    pub segment: Option<PlaybackSegment>,
    pub label: Option<String>,
    pub completed: bool,
    pub policy_state: PlaybackPolicyState,
    pub chapter_context: Option<ChapterPlaybackContext>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterPlaybackContext {
    pub episode_id: EpisodeId,
    pub artifact_id: ChapterArtifactId,
    pub selection_revision: StateRevision,
    pub session_id: ChapterPlaybackSessionId,
    pub policy_version: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PlaybackAllowedActions {
    pub can_play: bool,
    pub can_pause: bool,
    pub can_seek: bool,
    pub can_advance: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PlaybackPolicyState {
    Idle,
    AwaitingHost,
    Playing,
    Paused,
    Completed,
    Failed,
    Unsupported { wire_code: u32 },
}
