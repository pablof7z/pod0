use pod0_application::CommandEnvelope;
use pod0_domain::EpisodeId;
use pod0_storage::{PlaybackMutation, PlaybackObservationReaction};

use crate::runtime_playback_observation_reaction::{
    PlannedPlaybackObservation, PlaybackObservationUpdate,
};

pub(super) fn base() -> PlannedPlaybackObservation {
    PlannedPlaybackObservation {
        reaction: None,
        update: PlaybackObservationUpdate::Base,
    }
}

pub(super) fn reaction(
    root: &CommandEnvelope,
    label: &str,
    mutation: PlaybackMutation,
    effects: Vec<pod0_application::DurablePlaybackEffectRequest>,
) -> PlaybackObservationReaction {
    let envelope = super::runtime_playback_transitions::observation_action_envelope(root, label);
    storage_reaction(envelope.command_id, mutation, effects)
}

pub(super) fn storage_reaction(
    command_id: pod0_domain::CommandId,
    mutation: PlaybackMutation,
    effects: Vec<pod0_application::DurablePlaybackEffectRequest>,
) -> PlaybackObservationReaction {
    PlaybackObservationReaction {
        command_id,
        episode_id: crate::runtime_playback_apply::playback_episode_hint(&mutation, None),
        transition: crate::runtime_playback_apply::playback_transition(&mutation),
        mutation,
        effects,
    }
}

pub(super) fn combine_checkpoint_and_advance(
    checkpoint: PlaybackMutation,
    episode_id: EpisodeId,
) -> (PlaybackMutation, bool) {
    match checkpoint {
        PlaybackMutation::Checkpoint {
            position_milliseconds,
            ..
        } => (
            PlaybackMutation::CheckpointAndAdvanceQueue {
                episode_id,
                position_milliseconds,
            },
            true,
        ),
        _ => (PlaybackMutation::AdvanceQueue, false),
    }
}

pub(super) fn combine_checkpoint_and_finish(
    checkpoint: PlaybackMutation,
    episode_id: EpisodeId,
    suppress_auto_advance: bool,
) -> PlaybackMutation {
    match checkpoint {
        PlaybackMutation::Checkpoint {
            position_milliseconds,
            ..
        } => PlaybackMutation::CheckpointAndFinishActive {
            episode_id,
            position_milliseconds,
            suppress_auto_advance,
        },
        _ => PlaybackMutation::FinishActive {
            suppress_auto_advance,
        },
    }
}

pub(super) fn is_checkpoint(mutation: &PlaybackMutation) -> bool {
    matches!(
        mutation,
        PlaybackMutation::Checkpoint { .. }
            | PlaybackMutation::CheckpointAndAdvanceQueue { .. }
            | PlaybackMutation::CheckpointAndFinishActive { .. }
    )
}
