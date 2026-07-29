use pod0_application::{
    CommandEnvelope, CoreFailureCode, ExternalEpisodeInput, OperationResult, SyntheticPodcastInput,
};
use pod0_domain::{PodcastId, PodcastKind, PodcastRecord};

use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn upsert_synthetic_podcast(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        podcast: SyntheticPodcastInput,
    ) {
        if podcast.title.trim().is_empty() || podcast.categories.len() > 32 {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        }
        let podcast_id = podcast
            .podcast_id
            .unwrap_or_else(|| PodcastId::from_bytes(envelope.command_id.into_bytes()));
        let now = self.now();
        let record = PodcastRecord {
            podcast_id,
            kind: PodcastKind::Synthetic,
            feed_identity: None,
            title: podcast.title,
            author: podcast.author,
            image_url: podcast.image_url,
            description: podcast.description,
            language: podcast.language,
            categories: podcast.categories,
            discovered_at: now,
            title_is_placeholder: false,
            last_refreshed_at: None,
            etag: None,
            last_modified: None,
        };
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.upsert_synthetic_podcast(envelope.command_id, fingerprint, record, now.value)
            });
        match result {
            Ok(_) => match self.reload_listening() {
                Ok(()) => self.succeed(
                    envelope.command_id,
                    Some(OperationResult::Podcast { podcast_id }),
                ),
                Err(error) => self.fail(envelope.command_id, storage_failure(error)),
            },
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    pub(super) fn upsert_external_episode(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        episode: ExternalEpisodeInput,
    ) {
        let feed_identity = match episode.feed_url {
            Some(value) => match pod0_application::normalize_feed_url(&value) {
                Some(value) => Some(value),
                None => {
                    self.fail(envelope.command_id, CoreFailureCode::InvalidFeedUrl);
                    return;
                }
            },
            None => None,
        };
        let Some(audio_url) = pod0_application::normalize_media_url(&episode.audio_url) else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        };
        if episode.title.trim().is_empty() || episode.podcast_title.trim().is_empty() {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        }
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.upsert_external_episode(
                    envelope.command_id,
                    fingerprint,
                    episode.podcast_id,
                    feed_identity,
                    &episode.podcast_title,
                    &audio_url,
                    episode.guid.as_deref(),
                    &episode.title,
                    &episode.description,
                    episode.published_at.value,
                    episode.enclosure_mime_type.as_deref(),
                    episode.image_url.as_deref(),
                    episode.duration_milliseconds,
                    self.now().value,
                )
            });
        match result {
            Ok((_, resolved_podcast_id, episode_id)) => match self.reload_listening() {
                Ok(()) => self.succeed(
                    envelope.command_id,
                    Some(OperationResult::ExternalEpisode {
                        podcast_id: resolved_podcast_id,
                        episode_id,
                    }),
                ),
                Err(error) => self.fail(envelope.command_id, storage_failure(error)),
            },
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }
}
