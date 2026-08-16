use pod0_application::{
    CommandEnvelope, HostRequest, PlaybackHostState, PlaybackLifecycleObservation,
    PlaybackTransitionCue,
};
use pod0_domain::{AdSpanId, EpisodeId, PlaybackSeekReason};
use pod0_storage::{PlaybackMutation, PlaybackObservationReaction};

use crate::runtime_playback_observation_reaction_helpers::{
    base, combine_checkpoint_and_advance, combine_checkpoint_and_finish, reaction, storage_reaction,
};
use crate::runtime_state::FacadeState;

pub(super) struct PlannedPlaybackObservation {
    pub(super) reaction: Option<PlaybackObservationReaction>,
    pub(super) update: PlaybackObservationUpdate,
}

pub(super) enum PlaybackObservationUpdate {
    Ignore,
    Base,
    InterruptionBegan {
        checkpointed: bool,
    },
    InterruptionResumed {
        resumed: bool,
    },
    InterruptionPaused {
        checkpointed: bool,
    },
    MediaServicesReset {
        checkpointed: bool,
        episode_id: EpisodeId,
    },
    AutomaticAdSkip {
        ad_span_id: AdSpanId,
    },
    SegmentFinished {
        checkpointed: bool,
        next: Option<EpisodeId>,
    },
    EpisodeFinished {
        next: Option<EpisodeId>,
    },
    Checkpoint {
        committed: bool,
    },
    Unsupported,
}

impl FacadeState {
    pub(super) fn plan_playback_observation(
        &mut self,
        root: &CommandEnvelope,
        observed_at_ms: i64,
        value: &PlaybackLifecycleObservation,
    ) -> Option<PlannedPlaybackObservation> {
        if self
            .playback
            .position_command_fence_at_ms
            .is_some_and(|fence| observed_at_ms < fence)
        {
            return Some(PlannedPlaybackObservation {
                reaction: None,
                update: PlaybackObservationUpdate::Ignore,
            });
        }
        let active = self.listening.playback.active_episode_id;
        let prior = self.playback.last_observation.as_ref();
        let interruption_changed =
            prior.is_none_or(|previous| previous.interruption != value.interruption);
        let newly_ended = value.ended && prior.is_none_or(|previous| !previous.ended);
        if value.episode_id.is_some() && value.episode_id != active {
            return Some(base());
        }
        if interruption_changed
            && let Some(plan) = self.plan_interruption(root, observed_at_ms, value)
        {
            return Some(plan);
        }
        let Some(episode_id) = active.filter(|id| value.episode_id == Some(*id)) else {
            return Some(base());
        };
        if self.playback.completion_checkpoint_fence_episode_id == Some(episode_id) {
            return Some(base());
        }
        if pod0_domain::segment_reached(
            value.position_milliseconds,
            self.listening.playback.active_segment,
        ) {
            return self.plan_segment_finish(root, observed_at_ms, value, episode_id);
        }
        if newly_ended {
            return self.plan_episode_finish(root, observed_at_ms, value, episode_id);
        }
        if let Some(plan) = self.plan_automatic_ad_skip(root, value) {
            return Some(plan);
        }
        let mutation = self.checkpoint_mutation(
            episode_id,
            value.position_milliseconds,
            observed_at_ms,
            false,
        );
        let committed = !matches!(mutation, PlaybackMutation::ReceiptOnly);
        Some(PlannedPlaybackObservation {
            reaction: committed.then(|| reaction(root, "checkpoint", mutation, Vec::new())),
            update: PlaybackObservationUpdate::Checkpoint { committed },
        })
    }

    fn plan_segment_finish(
        &self,
        root: &CommandEnvelope,
        observed_at_ms: i64,
        value: &PlaybackLifecycleObservation,
        episode_id: EpisodeId,
    ) -> Option<PlannedPlaybackObservation> {
        let checkpoint = self.checkpoint_mutation(
            episode_id,
            value.position_milliseconds,
            observed_at_ms,
            true,
        );
        let (mutation, checkpointed) = combine_checkpoint_and_advance(checkpoint, episode_id);
        let next = self.listening.playback.queue.first().cloned();
        let envelope = super::runtime_playback_transitions::observation_action_envelope(
            root,
            "finish-segment",
        );
        let effects = next.as_ref().map_or_else(
            || {
                self.playback_effects(
                    &envelope,
                    vec![("segment-pause", HostRequest::Pause { episode_id })],
                )
            },
            |next| {
                self.plan_episode_load_effects(
                    &envelope,
                    next.episode_id,
                    next.segment,
                    true,
                    PlaybackTransitionCue::FadeIn {
                        duration_milliseconds: 250,
                    },
                )
            },
        )?;
        Some(PlannedPlaybackObservation {
            reaction: Some(storage_reaction(envelope.command_id, mutation, effects)),
            update: PlaybackObservationUpdate::SegmentFinished {
                checkpointed,
                next: next.map(|entry| entry.episode_id),
            },
        })
    }

    fn plan_episode_finish(
        &self,
        root: &CommandEnvelope,
        observed_at_ms: i64,
        value: &PlaybackLifecycleObservation,
        episode_id: EpisodeId,
    ) -> Option<PlannedPlaybackObservation> {
        let should_advance = !self.listening.playback.queue.is_empty()
            && self.listening.playback.auto_play_next
            && self.listening.playback.sleep_mode != pod0_domain::PlaybackSleepMode::EndOfEpisode
            && !self.playback.timer_fired;
        let next = should_advance
            .then(|| self.listening.playback.queue.first().cloned())
            .flatten();
        let checkpoint = self.checkpoint_mutation(
            episode_id,
            value.position_milliseconds,
            observed_at_ms,
            true,
        );
        let mutation =
            combine_checkpoint_and_finish(checkpoint, episode_id, self.playback.timer_fired);
        let envelope = super::runtime_playback_transitions::observation_action_envelope(
            root,
            "finish-episode",
        );
        let effects = next.as_ref().map_or_else(
            || Some(Vec::new()),
            |next| {
                self.plan_episode_load_effects(
                    &envelope,
                    next.episode_id,
                    next.segment,
                    true,
                    PlaybackTransitionCue::FadeIn {
                        duration_milliseconds: 250,
                    },
                )
            },
        )?;
        Some(PlannedPlaybackObservation {
            reaction: Some(storage_reaction(envelope.command_id, mutation, effects)),
            update: PlaybackObservationUpdate::EpisodeFinished {
                next: next.map(|entry| entry.episode_id),
            },
        })
    }

    fn plan_automatic_ad_skip(
        &self,
        root: &CommandEnvelope,
        value: &PlaybackLifecycleObservation,
    ) -> Option<PlannedPlaybackObservation> {
        let active = self.playback.chapter.as_ref()?;
        let decision = pod0_domain::decide_automatic_ad_skip(
            &active.artifact,
            value.position_milliseconds,
            self.playback.auto_skip_ads,
            value.state == PlaybackHostState::Playing,
            &self.playback.skipped_ad_span_ids,
        )?;
        let ad_span_id = decision.ad_span_id?;
        let envelope = super::runtime_playback_transitions::observation_action_envelope(
            root,
            "automatic-ad-skip",
        );
        let effects = self.playback_effects(
            &envelope,
            vec![(
                "automatic-ad-skip",
                HostRequest::Seek {
                    episode_id: active.context.episode_id,
                    position_milliseconds: decision.target_milliseconds,
                    reason: PlaybackSeekReason::AutomaticAdSkip,
                    chapter_context: Some(active.context),
                },
            )],
        )?;
        let mutation = PlaybackMutation::Checkpoint {
            episode_id: active.context.episode_id,
            position_milliseconds: decision.target_milliseconds,
        };
        Some(PlannedPlaybackObservation {
            reaction: Some(storage_reaction(envelope.command_id, mutation, effects)),
            update: PlaybackObservationUpdate::AutomaticAdSkip { ad_span_id },
        })
    }

    pub(super) fn observation_effects(
        &self,
        root: &CommandEnvelope,
        label: &'static str,
        request: HostRequest,
    ) -> Option<Vec<pod0_application::DurablePlaybackEffectRequest>> {
        let envelope =
            super::runtime_playback_transitions::observation_action_envelope(root, label);
        self.playback_effects(&envelope, vec![(label, request)])
    }
}
