use pod0_application::{
    CommandEnvelope, HostRequest, PlaybackInterruption, PlaybackLifecycleObservation,
    PlaybackTransitionCue,
};
use pod0_storage::PlaybackMutation;

use crate::runtime_playback_observation_reaction::{
    PlannedPlaybackObservation, PlaybackObservationUpdate,
};
use crate::runtime_playback_observation_reaction_helpers::{
    is_checkpoint, reaction, storage_reaction,
};
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn plan_interruption(
        &mut self,
        root: &CommandEnvelope,
        observed_at_ms: i64,
        value: &PlaybackLifecycleObservation,
    ) -> Option<PlannedPlaybackObservation> {
        let episode_id = self.listening.playback.active_episode_id?;
        match value.interruption {
            PlaybackInterruption::None => None,
            PlaybackInterruption::Began => {
                let mutation = self.checkpoint_mutation(
                    episode_id,
                    value.position_milliseconds,
                    observed_at_ms,
                    true,
                );
                let effects = self.observation_effects(
                    root,
                    "interruption-began",
                    HostRequest::Pause { episode_id },
                )?;
                Some(PlannedPlaybackObservation {
                    reaction: Some(reaction(
                        root,
                        "interruption-began",
                        mutation.clone(),
                        effects,
                    )),
                    update: PlaybackObservationUpdate::InterruptionBegan {
                        checkpointed: is_checkpoint(&mutation),
                    },
                })
            }
            PlaybackInterruption::EndedShouldResume => {
                let resumed = self.playback.desired_playing
                    && self.playback.interrupted_episode_id == Some(episode_id)
                    && !value.ended;
                let reaction = if resumed {
                    let effects = self.observation_effects(
                        root,
                        "interruption-resume",
                        HostRequest::Play {
                            episode_id,
                            transition_cue: PlaybackTransitionCue::Immediate,
                        },
                    )?;
                    Some(reaction(
                        root,
                        "interruption-resume",
                        PlaybackMutation::ReceiptOnly,
                        effects,
                    ))
                } else {
                    None
                };
                Some(PlannedPlaybackObservation {
                    reaction,
                    update: PlaybackObservationUpdate::InterruptionResumed { resumed },
                })
            }
            PlaybackInterruption::EndedShouldRemainPaused | PlaybackInterruption::RouteLost => {
                let mutation = self.checkpoint_mutation(
                    episode_id,
                    value.position_milliseconds,
                    observed_at_ms,
                    true,
                );
                let effects = self.observation_effects(
                    root,
                    "interruption-paused",
                    HostRequest::Pause { episode_id },
                )?;
                Some(PlannedPlaybackObservation {
                    reaction: Some(reaction(
                        root,
                        "interruption-paused",
                        mutation.clone(),
                        effects,
                    )),
                    update: PlaybackObservationUpdate::InterruptionPaused {
                        checkpointed: is_checkpoint(&mutation),
                    },
                })
            }
            PlaybackInterruption::MediaServicesReset => {
                let mutation = self.checkpoint_mutation(
                    episode_id,
                    value.position_milliseconds,
                    observed_at_ms,
                    true,
                );
                let envelope = super::runtime_playback_transitions::observation_action_envelope(
                    root,
                    "media-services-reset",
                );
                let effects = self.plan_active_load_effects(
                    &envelope,
                    self.playback.desired_playing && !value.ended,
                    PlaybackTransitionCue::Immediate,
                )?;
                Some(PlannedPlaybackObservation {
                    reaction: Some(storage_reaction(
                        envelope.command_id,
                        mutation.clone(),
                        effects,
                    )),
                    update: PlaybackObservationUpdate::MediaServicesReset {
                        checkpointed: is_checkpoint(&mutation),
                        episode_id,
                    },
                })
            }
            PlaybackInterruption::Unsupported { .. } => Some(PlannedPlaybackObservation {
                reaction: None,
                update: PlaybackObservationUpdate::Unsupported,
            }),
        }
    }
}
