use pod0_domain::{
    CommandId, FeedIdentityV1, HostRequestId, PodcastId, PodcastKind, PodcastRecord,
    UnixTimestampMilliseconds,
};
use rusqlite::{OptionalExtension, Transaction};

use crate::StorageError;
use crate::download_store_request::derived_request_id;
use crate::feed_fetch_store_model::{FeedFetchEnsureInput, FeedFetchEnsureOutcome};
use crate::library_store::LibraryStore;

impl LibraryStore {
    /// Commits the feed intent durably in one transaction: the placeholder
    /// podcast, the subscription when subscribing, and the workflow row the
    /// host request is issued from. The `feed_key_v1` primary key coalesces
    /// concurrent intents for one normalized feed identity onto one workflow.
    pub fn ensure_feed_fetch_workflow(
        &self,
        input: FeedFetchEnsureInput,
    ) -> Result<FeedFetchEnsureOutcome, StorageError> {
        self.commit_feed_fetch_admission(input)
    }
}

pub(crate) fn feed_fetch_request_id(
    feed_key: &str,
    command_id: CommandId,
    attempt: u16,
) -> HostRequestId {
    let mut identity = Vec::with_capacity(feed_key.len() + 16);
    identity.extend_from_slice(feed_key.as_bytes());
    identity.extend_from_slice(&command_id.into_bytes());
    derived_request_id(b"pod0-feed-fetch-request-v1", &identity, u64::from(attempt))
}

pub(crate) fn placeholder_podcast(
    input: &FeedFetchEnsureInput,
    podcast_id: PodcastId,
) -> PodcastRecord {
    PodcastRecord {
        podcast_id,
        kind: PodcastKind::Rss,
        feed_identity: Some(FeedIdentityV1 {
            source_url: input.source_url.clone(),
            comparison_key: input.feed_key.clone(),
        }),
        title: input.placeholder_title.clone(),
        author: String::new(),
        image_url: None,
        description: String::new(),
        language: None,
        categories: Vec::new(),
        discovered_at: UnixTimestampMilliseconds::new(input.now_ms),
        title_is_placeholder: true,
        last_refreshed_at: None,
        etag: None,
        last_modified: None,
    }
}

pub(crate) fn podcast_id_for_feed_key(
    transaction: &Transaction<'_>,
    feed_key: &str,
) -> Result<Option<PodcastId>, StorageError> {
    let stored: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT podcast_id FROM pod0_podcasts WHERE feed_key_v1=?1",
            [feed_key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("resolve feed fetch podcast", error))?;
    stored
        .map(|bytes| {
            let bytes: [u8; 16] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
                detail: "feed fetch podcast identity is malformed",
            })?;
            Ok(PodcastId::from_bytes(bytes))
        })
        .transpose()
}
