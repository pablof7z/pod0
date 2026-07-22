use pod0_application::{
    ApplicationCommand, CommandEnvelope, CoreFailureCode, DownloadIntentOrigin,
};
use pod0_domain::{AutoDownloadMode, CancellationId, CommandId, EpisodeId, PodcastId};
use sha2::{Digest as _, Sha256};

use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn report_automatic_download_candidates(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        podcast_id: PodcastId,
        episode_ids: Vec<EpisodeId>,
    ) {
        const MAXIMUM_CANDIDATES: usize = 200;
        let Some(store) = self.store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        if store.require_download_workflow_authoritative().is_err() {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        }
        let mut unique = std::collections::BTreeSet::new();
        if episode_ids.len() > MAXIMUM_CANDIDATES
            || episode_ids
                .iter()
                .any(|episode_id| !unique.insert(*episode_id))
        {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        }
        if let Err(error) = self.reload_listening() {
            self.fail(envelope.command_id, storage_failure(error));
            return;
        }
        let mut candidates = Vec::with_capacity(episode_ids.len());
        for episode_id in episode_ids {
            let Some(episode) = self
                .listening
                .episodes
                .iter()
                .find(|episode| {
                    episode.episode_id == episode_id && episode.podcast_id == podcast_id
                })
                .cloned()
            else {
                self.fail(envelope.command_id, CoreFailureCode::NotFound);
                return;
            };
            candidates.push(episode);
        }
        candidates.sort_by(|left, right| {
            right
                .published_at
                .value
                .cmp(&left.published_at.value)
                .then_with(|| left.episode_id.cmp(&right.episode_id))
        });
        let policy = self.download_policy(podcast_id);
        let selected = match policy.mode {
            AutoDownloadMode::Off | AutoDownloadMode::Unsupported { .. } => 0,
            AutoDownloadMode::Latest { count } => usize::from(count).min(candidates.len()),
            AutoDownloadMode::AllNew => candidates.len(),
        };
        if let Err(error) =
            store.record_download_noop_command(envelope.command_id, fingerprint, self.now().value)
        {
            self.fail(envelope.command_id, storage_failure(error));
            return;
        }
        for episode in candidates.into_iter().take(selected) {
            let command = ApplicationCommand::RequestEpisodeDownload {
                episode_id: episode.episode_id,
                origin: DownloadIntentOrigin::Automatic,
            };
            let child = CommandEnvelope {
                command_id: derived_command_id(
                    b"pod0-auto-download-command-v1",
                    envelope.command_id,
                    episode.episode_id,
                ),
                cancellation_id: CancellationId::from_bytes(derived_bytes(
                    b"pod0-auto-download-cancellation-v1",
                    envelope.command_id,
                    episode.episode_id,
                )),
                expected_revision: None,
                command,
            };
            self.begin(&child);
            let child_fingerprint =
                crate::runtime_command_fingerprint::command_fingerprint(&child.command);
            self.request_episode_download(
                &child,
                &child_fingerprint,
                episode.episode_id,
                DownloadIntentOrigin::Automatic,
            );
        }
        self.succeed(envelope.command_id, None);
    }
}

fn derived_command_id(domain: &[u8], parent: CommandId, episode: EpisodeId) -> CommandId {
    CommandId::from_bytes(derived_bytes(domain, parent, episode))
}

fn derived_bytes(domain: &[u8], parent: CommandId, episode: EpisodeId) -> [u8; 16] {
    let mut hash = Sha256::new();
    hash.update((domain.len() as u64).to_be_bytes());
    hash.update(domain);
    hash.update(parent.into_bytes());
    hash.update(episode.into_bytes());
    hash.finalize()[..16].try_into().expect("digest prefix")
}
