use pod0_application::{
    CommandEnvelope, CoreFailureCode, ExternalEpisodeInput, HostRequest, HostRequestEnvelope,
    MAX_FEED_RESPONSE_BYTES, OperationResult, OperationStage, SyntheticPodcastInput,
};
use pod0_domain::{
    HostRequestId, PodcastId, PodcastKind, PodcastRecord, UnixTimestampMilliseconds,
};

use crate::runtime_commands::storage_failure;
use crate::runtime_state::{FacadeState, FeedIntent, PendingFeed};

impl FacadeState {
    pub(super) fn start_feed(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        feed_url: String,
        intent: FeedIntent,
    ) {
        let Some(identity) = pod0_application::normalize_feed_url(&feed_url) else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidFeedUrl);
            return;
        };
        let existing = self.listening.podcasts.iter().find(|podcast| {
            podcast
                .feed_identity
                .as_ref()
                .is_some_and(|feed| feed.comparison_key == identity.comparison_key)
        });
        if intent == FeedIntent::Subscribe
            && existing.is_some_and(|podcast| {
                self.listening
                    .subscriptions
                    .iter()
                    .any(|row| row.podcast_id == podcast.podcast_id)
            })
        {
            self.fail(envelope.command_id, CoreFailureCode::AlreadySubscribed);
            return;
        }
        if intent == FeedIntent::Ensure
            && let Some(podcast) = existing
        {
            self.succeed(
                envelope.command_id,
                Some(OperationResult::Podcast {
                    podcast_id: podcast.podcast_id,
                }),
            );
            return;
        }
        let podcast_id = existing.map_or_else(
            || PodcastId::from_bytes(envelope.command_id.into_bytes()),
            |podcast| podcast.podcast_id,
        );
        self.queue_feed_request(
            envelope,
            fingerprint,
            intent,
            identity,
            podcast_id,
            None,
            None,
        );
    }

    pub(super) fn start_refresh(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        podcast_id: PodcastId,
    ) {
        let Some(podcast) = self
            .listening
            .podcasts
            .iter()
            .find(|podcast| podcast.podcast_id == podcast_id)
        else {
            self.fail(envelope.command_id, CoreFailureCode::NotFound);
            return;
        };
        let Some(identity) = podcast.feed_identity.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidFeedUrl);
            return;
        };
        self.queue_feed_request(
            envelope,
            fingerprint,
            FeedIntent::Refresh,
            identity,
            podcast_id,
            podcast.etag.clone(),
            podcast.last_modified.clone(),
        );
    }

    pub(super) fn start_metadata_refresh(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        podcast_id: PodcastId,
    ) {
        let Some(podcast) = self
            .listening
            .podcasts
            .iter()
            .find(|podcast| podcast.podcast_id == podcast_id)
        else {
            self.fail(envelope.command_id, CoreFailureCode::NotFound);
            return;
        };
        let Some(identity) = podcast.feed_identity.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidFeedUrl);
            return;
        };
        self.queue_feed_request(
            envelope,
            fingerprint,
            FeedIntent::Metadata,
            identity,
            podcast_id,
            podcast.etag.clone(),
            podcast.last_modified.clone(),
        );
    }

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

    #[allow(clippy::too_many_arguments)]
    fn queue_feed_request(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        intent: FeedIntent,
        identity: pod0_domain::FeedIdentityV1,
        podcast_id: PodcastId,
        entity_tag: Option<String>,
        last_modified: Option<String>,
    ) {
        let request_id = HostRequestId::from_bytes(envelope.command_id.into_bytes());
        let request = HostRequestEnvelope {
            request_id,
            command_id: envelope.command_id,
            cancellation_id: envelope.cancellation_id,
            issued_revision: self.revision,
            deadline_at: Some(UnixTimestampMilliseconds::new(
                self.now().value.saturating_add(30_000),
            )),
            request: HostRequest::FetchFeed {
                feed_url: identity.source_url.clone(),
                entity_tag,
                last_modified,
                maximum_response_bytes: MAX_FEED_RESPONSE_BYTES,
            },
        };
        if self.host_requests.register(request.clone()) {
            self.pending_feeds.insert(
                request_id,
                PendingFeed {
                    command_id: envelope.command_id,
                    fingerprint: fingerprint.to_owned(),
                    intent,
                    feed_identity: identity,
                    podcast_id,
                },
            );
            self.host_queue.push_back(request);
            self.finish(envelope.command_id, OperationStage::Running, None, None);
        } else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
        }
    }
}
