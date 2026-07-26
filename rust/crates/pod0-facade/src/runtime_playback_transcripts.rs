use pod0_application::{
    ApplicationCommand, CommandEnvelope, TranscriptWorkflowConfiguration, TranscriptWorkflowOrigin,
};
use pod0_domain::{CancellationId, CommandId, EpisodeId, TranscriptStartPolicy};
use sha2::{Digest as _, Sha256};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn transcript_origin_is_allowed(
        &self,
        episode_id: EpisodeId,
        origin: TranscriptWorkflowOrigin,
    ) -> bool {
        if origin == TranscriptWorkflowOrigin::User {
            return true;
        }
        let Some(episode) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
        else {
            return false;
        };
        let policy = self
            .listening
            .subscriptions
            .iter()
            .find(|subscription| subscription.podcast_id == episode.podcast_id)
            .map_or(TranscriptStartPolicy::Automatic, |subscription| {
                subscription.transcript_start_policy
            });
        matches!(
            (origin, policy),
            (
                TranscriptWorkflowOrigin::Automatic,
                TranscriptStartPolicy::Automatic
            ) | (
                TranscriptWorkflowOrigin::Playback,
                TranscriptStartPolicy::WhenPlayed
            )
        )
    }

    pub(super) fn start_playback_transcript_if_needed(
        &mut self,
        parent: &CommandEnvelope,
        episode_id: EpisodeId,
        configuration: Option<TranscriptWorkflowConfiguration>,
    ) {
        let Some(configuration) = configuration else {
            return;
        };
        if !self.transcript_origin_is_allowed(episode_id, TranscriptWorkflowOrigin::Playback) {
            return;
        }
        let child = CommandEnvelope {
            command_id: CommandId::from_bytes(derived_bytes(
                b"pod0-playback-transcript-command-v1",
                parent.command_id,
                episode_id,
            )),
            cancellation_id: CancellationId::from_bytes(derived_bytes(
                b"pod0-playback-transcript-cancellation-v1",
                parent.command_id,
                episode_id,
            )),
            expected_revision: None,
            command: ApplicationCommand::EnsureTranscriptWorkflow {
                episode_id,
                origin: TranscriptWorkflowOrigin::Playback,
                configuration: configuration.clone(),
            },
        };
        self.begin(&child);
        self.ensure_transcript_workflow(
            &child,
            episode_id,
            TranscriptWorkflowOrigin::Playback,
            configuration,
        );
    }
}

fn derived_bytes(domain: &[u8], parent: CommandId, episode: EpisodeId) -> [u8; 16] {
    let mut hash = Sha256::new();
    hash.update((domain.len() as u64).to_be_bytes());
    hash.update(domain);
    hash.update(parent.into_bytes());
    hash.update(episode.into_bytes());
    hash.finalize()[..16].try_into().expect("digest prefix")
}
