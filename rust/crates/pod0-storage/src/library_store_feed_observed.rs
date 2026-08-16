use std::collections::BTreeMap;

use pod0_domain::{CommandId, EpisodeRecord, PodcastRecord};
use rusqlite::Transaction;

use crate::StorageError;
use crate::feed_discovery_store::{NewFeedDiscoveryItem, insert_apply_receipt, insert_occurrence};
use crate::feed_discovery_store_model::AppliedFeed;
use crate::library_store_feed::{
    episode_id, insert_subscription, podcast_has_episodes, resolve_podcast_id, upsert_episode,
    upsert_podcast,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_observed_feed(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    mut podcast: PodcastRecord,
    mut episodes: Vec<EpisodeRecord>,
    subscribe: bool,
    record_discovery: bool,
    entity_tag: Option<String>,
    last_modified: Option<String>,
    observed_at_ms: i64,
    revision: pod0_domain::StateRevision,
) -> Result<AppliedFeed, StorageError> {
    let podcast_id = resolve_podcast_id(transaction, &podcast)?;
    if podcast_id != podcast.podcast_id {
        podcast.podcast_id = podcast_id;
        for episode in &mut episodes {
            episode.podcast_id = podcast_id;
            episode.episode_id = episode_id(podcast_id, &episode.publisher_guid);
        }
    }
    let is_initial_population = !podcast_has_episodes(transaction, podcast_id)?;
    podcast.etag = entity_tag.or(podcast.etag);
    podcast.last_modified = last_modified.or(podcast.last_modified);
    upsert_podcast(transaction, &podcast)?;
    let mut inserted = BTreeMap::new();
    for episode in &episodes {
        let (episode_id, was_inserted) = upsert_episode(transaction, episode)?;
        if was_inserted || inserted.contains_key(&episode_id) {
            inserted.insert(
                episode_id,
                NewFeedDiscoveryItem {
                    episode_id,
                    input_version: pod0_application::feed_discovery_item_input_version(episode),
                    published_at_ms: episode.published_at.value,
                },
            );
        }
    }
    if subscribe {
        insert_subscription(transaction, podcast_id, observed_at_ms)?;
    }
    let inserted_episode_count =
        u32::try_from(inserted.len()).map_err(|_| StorageError::CorruptSchema {
            detail: "feed discovery item count overflows",
        })?;
    let discovery_occurrence_id = if !record_discovery || inserted.is_empty() {
        None
    } else {
        Some(insert_occurrence(
            transaction,
            command_id,
            podcast_id,
            is_initial_population,
            observed_at_ms,
            &inserted.into_values().collect::<Vec<_>>(),
        )?)
    };
    let applied = AppliedFeed {
        revision,
        podcast_id,
        discovery_occurrence_id,
        inserted_episode_count,
    };
    insert_apply_receipt(transaction, command_id, &applied)?;
    Ok(applied)
}
