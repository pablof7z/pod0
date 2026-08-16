use pod0_application::{
    FeedFetchIntent, FeedFetchProjection, FeedFetchStage, MAX_ACTIVE_FEED_FETCH_WORKFLOWS,
};
use pod0_domain::UnixTimestampMilliseconds;
use pod0_storage::{StoredFeedFetchIntent, StoredFeedFetchStage};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn rehydrate_feed_workflows(&mut self) -> Result<(), pod0_storage::StorageError> {
        self.reload_feed_fetches()
    }

    pub(super) fn reload_feed_fetches(&mut self) -> Result<(), pod0_storage::StorageError> {
        self.feed_fetches = self
            .store
            .as_ref()
            .map(|store| store.feed_fetch_workflows_snapshot(MAX_ACTIVE_FEED_FETCH_WORKFLOWS))
            .transpose()?
            .unwrap_or_default();
        Ok(())
    }

    pub(super) fn feed_fetch_projection(&self) -> Vec<FeedFetchProjection> {
        self.feed_fetches
            .iter()
            .map(|record| FeedFetchProjection {
                podcast_id: record.podcast_id,
                feed_url: record.source_url.clone(),
                intent: match record.intent {
                    StoredFeedFetchIntent::Subscribe => FeedFetchIntent::Subscribe,
                    StoredFeedFetchIntent::Ensure => FeedFetchIntent::Ensure,
                    StoredFeedFetchIntent::Refresh => FeedFetchIntent::Refresh,
                    StoredFeedFetchIntent::Metadata => FeedFetchIntent::Metadata,
                },
                stage: match record.stage {
                    StoredFeedFetchStage::Requested => FeedFetchStage::Requested,
                    StoredFeedFetchStage::RetryScheduled => FeedFetchStage::RetryScheduled,
                    StoredFeedFetchStage::Failed => FeedFetchStage::Failed,
                },
                attempt: record.attempt,
                request_id: record.request_id,
                not_before: record.not_before_ms.map(UnixTimestampMilliseconds::new),
                failure_code: record.failure_code.clone(),
                updated_at: UnixTimestampMilliseconds::new(record.updated_at_ms),
            })
            .collect()
    }
}
