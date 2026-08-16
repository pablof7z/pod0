use pod0_application::{
    CommandEnvelope, CoreFailureCode, DurableInternalCommandRequest, HostRequest, OperationResult,
};
use pod0_domain::EpisodeId;
use pod0_storage::PlaybackMutation;

use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn apply_playback_command(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        mutation: PlaybackMutation,
        result: OperationResult,
    ) -> bool {
        self.apply_playback_command_with_internal_and_effects(
            envelope,
            fingerprint,
            mutation,
            result,
            None,
            Vec::new(),
        )
    }

    pub(super) fn apply_playback_command_with_effects(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        mutation: PlaybackMutation,
        result: OperationResult,
        effects: Vec<(&str, HostRequest)>,
    ) -> bool {
        let Some(effects) = self.playback_effects(envelope, effects) else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return false;
        };
        self.apply_playback_command_with_internal_and_effects(
            envelope,
            fingerprint,
            mutation,
            result,
            None,
            effects,
        )
    }

    pub(super) fn apply_playback_command_with_durable_effects(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        mutation: PlaybackMutation,
        result: OperationResult,
        effects: Vec<pod0_application::DurablePlaybackEffectRequest>,
    ) -> bool {
        self.apply_playback_command_with_internal_and_effects(
            envelope,
            fingerprint,
            mutation,
            result,
            None,
            effects,
        )
    }

    pub(super) fn apply_playback_command_with_internal_and_effects(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        mutation: PlaybackMutation,
        result: OperationResult,
        internal_command: Option<DurableInternalCommandRequest>,
        effects: Vec<pod0_application::DurablePlaybackEffectRequest>,
    ) -> bool {
        let episode_id =
            playback_episode_hint(&mutation, self.listening.playback.active_episode_id);
        let transition = playback_transition(&mutation);
        let outcome = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.apply_playback_mutation(
                    envelope.command_id,
                    fingerprint,
                    mutation,
                    episode_id,
                    transition,
                    internal_command,
                    effects,
                    self.now().value,
                )
            });
        match outcome {
            Ok(_) => match self.reload_listening() {
                Ok(()) => {
                    self.succeed(envelope.command_id, Some(result));
                    true
                }
                Err(error) => {
                    self.fail(envelope.command_id, storage_failure(error));
                    false
                }
            },
            Err(error) => {
                self.fail(envelope.command_id, storage_failure(error));
                false
            }
        }
    }
}

pub(super) fn playback_episode_hint(
    mutation: &PlaybackMutation,
    active: Option<EpisodeId>,
) -> Option<EpisodeId> {
    match mutation {
        PlaybackMutation::Select { episode_id, .. }
        | PlaybackMutation::SetCompletion { episode_id, .. }
        | PlaybackMutation::Checkpoint { episode_id, .. }
        | PlaybackMutation::CheckpointAndAdvanceQueue { episode_id, .. }
        | PlaybackMutation::CheckpointAndFinishActive { episode_id, .. } => Some(*episode_id),
        PlaybackMutation::Enqueue { entry, .. } => Some(entry.episode_id),
        PlaybackMutation::RemoveEpisode(episode_id)
        | PlaybackMutation::ResetProgress(episode_id) => Some(*episode_id),
        _ => active,
    }
}

pub(super) fn playback_transition(
    mutation: &PlaybackMutation,
) -> pod0_application::PlaybackTransition {
    use pod0_application::PlaybackTransition;
    match mutation {
        PlaybackMutation::Enqueue { .. }
        | PlaybackMutation::RemoveQueueEntry(_)
        | PlaybackMutation::RemoveEpisode(_)
        | PlaybackMutation::ReplaceQueueOrder(_)
        | PlaybackMutation::ClearQueue
        | PlaybackMutation::AdvanceQueue
        | PlaybackMutation::CheckpointAndAdvanceQueue { .. } => PlaybackTransition::QueueChanged,
        PlaybackMutation::SetRate(_) => PlaybackTransition::RateChanged,
        PlaybackMutation::SetSleepTimer(_) => PlaybackTransition::SleepTimerChanged,
        PlaybackMutation::Checkpoint { .. } | PlaybackMutation::ResetProgress(_) => {
            PlaybackTransition::PositionCheckpointCommitted
        }
        _ => PlaybackTransition::SessionStateChanged,
    }
}
