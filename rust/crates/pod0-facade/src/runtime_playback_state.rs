use std::collections::BTreeSet;

use pod0_application::{
    ChapterPlaybackContext, PlaybackHostState, PlaybackLifecycleObservation, PlaybackPolicyState,
};
use pod0_domain::{AdSpanId, ChapterArtifact, EpisodeId, HostRequestId};

#[derive(Clone, Debug)]
pub(super) struct PlaybackRuntime {
    pub(super) policy_state: PlaybackPolicyState,
    pub(super) host_state: PlaybackHostState,
    pub(super) desired_playing: bool,
    pub(super) media_episode_id: Option<EpisodeId>,
    pub(super) interrupted_episode_id: Option<EpisodeId>,
    pub(super) observation_request_id: Option<HostRequestId>,
    pub(super) last_observation: Option<PlaybackLifecycleObservation>,
    pub(super) last_position_commit_at_ms: Option<i64>,
    pub(super) position_command_fence_at_ms: Option<i64>,
    pub(super) timer_fired: bool,
    pub(super) chapter: Option<ActiveChapterPlayback>,
    pub(super) auto_skip_ads: bool,
    pub(super) skipped_ad_span_ids: BTreeSet<AdSpanId>,
}

#[derive(Clone, Debug)]
pub(super) struct ActiveChapterPlayback {
    pub(super) context: ChapterPlaybackContext,
    pub(super) artifact: ChapterArtifact,
}

impl Default for PlaybackRuntime {
    fn default() -> Self {
        Self {
            policy_state: PlaybackPolicyState::Idle,
            host_state: PlaybackHostState::Idle,
            desired_playing: false,
            media_episode_id: None,
            interrupted_episode_id: None,
            observation_request_id: None,
            last_observation: None,
            last_position_commit_at_ms: None,
            position_command_fence_at_ms: None,
            timer_fired: false,
            chapter: None,
            auto_skip_ads: false,
            skipped_ad_span_ids: BTreeSet::new(),
        }
    }
}
