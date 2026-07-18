use pod0_application::{
    CommandEnvelope, CoreFailureCode, HostRequest, HostRequestEnvelope, MAX_FEED_RESPONSE_BYTES,
    OperationResult, OperationStage,
};
use pod0_domain::{HostRequestId, PodcastId, UnixTimestampMilliseconds};

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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn upsert_external_episode(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        podcast_id: PodcastId,
        feed_url: Option<String>,
        podcast_title: String,
        audio_url: String,
        title: String,
        image_url: Option<String>,
        duration_milliseconds: Option<u64>,
    ) {
        let feed_identity = match feed_url {
            Some(value) => match pod0_application::normalize_feed_url(&value) {
                Some(value) => Some(value),
                None => {
                    self.fail(envelope.command_id, CoreFailureCode::InvalidFeedUrl);
                    return;
                }
            },
            None => None,
        };
        let Some(audio_identity) = pod0_application::normalize_feed_url(&audio_url) else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        };
        if title.trim().is_empty() {
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
                    podcast_id,
                    feed_identity,
                    &podcast_title,
                    &audio_identity.source_url,
                    &title,
                    image_url.as_deref(),
                    duration_milliseconds,
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
