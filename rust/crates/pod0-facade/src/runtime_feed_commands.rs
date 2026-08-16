use pod0_application::{
    CommandEnvelope, CoreFailureCode, FEED_FETCH_HOST_REQUEST_DEADLINE_MILLISECONDS,
    OperationResult,
};
use pod0_domain::PodcastId;
use pod0_storage::{FeedFetchEnsureInput, StoredFeedFetchIntent};

use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn start_feed(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        feed_url: String,
        intent: StoredFeedFetchIntent,
    ) {
        let Some(identity) = pod0_application::normalize_feed_url(&feed_url) else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidFeedUrl);
            return;
        };
        let existing = self
            .listening
            .podcasts
            .iter()
            .find(|podcast| {
                podcast
                    .feed_identity
                    .as_ref()
                    .is_some_and(|feed| feed.comparison_key == identity.comparison_key)
            })
            .map(|podcast| podcast.podcast_id);
        let workflow_active = self.feed_fetches.iter().any(|record| {
            record.feed_key == identity.comparison_key
                && record.stage != pod0_storage::StoredFeedFetchStage::Failed
        });
        if intent == StoredFeedFetchIntent::Subscribe
            && !workflow_active
            && existing.is_some_and(|podcast_id| {
                self.listening
                    .subscriptions
                    .iter()
                    .any(|row| row.podcast_id == podcast_id)
            })
        {
            self.fail(envelope.command_id, CoreFailureCode::AlreadySubscribed);
            return;
        }
        if intent == StoredFeedFetchIntent::Ensure
            && let Some(podcast_id) = existing
        {
            self.succeed(
                envelope.command_id,
                Some(OperationResult::Podcast { podcast_id }),
            );
            return;
        }
        let podcast_id =
            existing.unwrap_or_else(|| PodcastId::from_bytes(envelope.command_id.into_bytes()));
        self.commit_feed_workflow(
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
        self.start_conditional_fetch(
            envelope,
            fingerprint,
            podcast_id,
            StoredFeedFetchIntent::Refresh,
        );
    }

    pub(super) fn start_metadata_refresh(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        podcast_id: PodcastId,
    ) {
        self.start_conditional_fetch(
            envelope,
            fingerprint,
            podcast_id,
            StoredFeedFetchIntent::Metadata,
        );
    }

    fn start_conditional_fetch(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        podcast_id: PodcastId,
        intent: StoredFeedFetchIntent,
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
        let entity_tag = podcast.etag.clone();
        let last_modified = podcast.last_modified.clone();
        self.commit_feed_workflow(
            envelope,
            fingerprint,
            intent,
            identity,
            podcast_id,
            entity_tag,
            last_modified,
        );
    }

    /// Commits the intent durably (placeholder podcast, subscription, and
    /// workflow row in one transaction), queues the host fetch from the
    /// stored row, and succeeds immediately: from contract version 53 a
    /// `Succeeded` feed command means "durably queued", not "fully applied".
    #[allow(clippy::too_many_arguments)]
    fn commit_feed_workflow(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        intent: StoredFeedFetchIntent,
        identity: pod0_domain::FeedIdentityV1,
        podcast_id: PodcastId,
        entity_tag: Option<String>,
        last_modified: Option<String>,
    ) {
        let Some(store) = self.store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let now = self.now().value;
        let Some(deadline) = now.checked_add(FEED_FETCH_HOST_REQUEST_DEADLINE_MILLISECONDS) else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        };
        let outcome = store.ensure_feed_fetch_workflow(FeedFetchEnsureInput {
            command_id: envelope.command_id,
            command_fingerprint: fingerprint.to_owned(),
            cancellation_id: envelope.cancellation_id,
            source_url: identity.source_url.clone(),
            feed_key: identity.comparison_key.clone(),
            podcast_id,
            placeholder_title: pod0_application::feed_placeholder_title(&identity),
            intent,
            entity_tag,
            last_modified,
            issued_revision: self.revision,
            now_ms: now,
            deadline_at_ms: deadline,
        });
        match outcome {
            Ok(outcome) => {
                if let Err(error) = self.reload_listening() {
                    self.fail(envelope.command_id, storage_failure(error));
                    return;
                }
                let _ = self.reload_feed_fetches();
                self.succeed(
                    envelope.command_id,
                    Some(OperationResult::Podcast {
                        podcast_id: outcome.podcast_id,
                    }),
                );
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }
}
