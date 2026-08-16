use pod0_application::ResolvedSharedEpisode;
use pod0_domain::EpisodeId;
use rusqlite::params;

use crate::{StorageError, library_store::source_import_id, library_store_feed::episode_id};

pub(crate) fn apply_resolved_shared_episode(
    transaction: &rusqlite::Transaction<'_>,
    episode: &ResolvedSharedEpisode,
    observed_at_ms: i64,
) -> Result<EpisodeId, StorageError> {
    let feed = episode
        .feed_url
        .as_deref()
        .and_then(pod0_application::normalize_feed_url);
    let parent = crate::library_store_external::ensure_external_parent(
        transaction,
        episode.podcast_id,
        feed,
        &episode.podcast_title,
        observed_at_ms,
    )?;
    let guid = episode
        .guid
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&episode.audio_url);
    let proposed = episode_id(parent, guid);
    let origin = source_import_id(transaction)?;
    let duration = episode
        .duration_milliseconds
        .map(|value| crate::listening_db_codec::i64_value(value, "shared episode duration"))
        .transpose()?;
    transaction.execute(
        "INSERT INTO pod0_episodes(episode_id,podcast_id,publisher_guid,title,description,\
         published_at_ms,duration_ms,enclosure_url,enclosure_mime_type,image_url,resume_position_ms,\
         completion_code,is_starred,download_code,transcript_code,legacy_payload,source_import_id) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,0,1,0,1,1,x'7b7d',?11) \
         ON CONFLICT(podcast_id,publisher_guid) DO UPDATE SET \
         title=CASE WHEN excluded.title='' THEN pod0_episodes.title ELSE excluded.title END,\
         description=excluded.description,duration_ms=COALESCE(excluded.duration_ms,pod0_episodes.duration_ms),\
         enclosure_mime_type=COALESCE(excluded.enclosure_mime_type,pod0_episodes.enclosure_mime_type),\
         image_url=COALESCE(excluded.image_url,pod0_episodes.image_url),enclosure_url=excluded.enclosure_url",
        params![proposed.into_bytes().as_slice(), parent.into_bytes().as_slice(), guid,
            episode.title, episode.description, episode.published_at_milliseconds, duration,
            episode.audio_url, episode.enclosure_mime_type, episode.image_url, origin],
    ).map_err(|error| StorageError::sqlite("upsert resolved shared episode", error))?;
    let actual = crate::library_store_external::find_episode_id(transaction, parent, guid)?;
    transaction
        .execute(
            "INSERT INTO pod0_episode_feed_metadata(episode_id,persons_json,sound_bites_json) \
         VALUES(?1,'[]','[]') ON CONFLICT(episode_id) DO NOTHING",
            [actual.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("initialize shared episode metadata", error))?;
    Ok(actual)
}

pub(crate) fn apply_catalog_results(
    transaction: &rusqlite::Transaction<'_>,
    candidates: Vec<pod0_application::CatalogEpisodeCandidate>,
    observed_at_ms: i64,
) -> Result<crate::StoredLibraryNetworkResult, StorageError> {
    let mut episode_ids = Vec::with_capacity(candidates.len());
    let mut rows = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let episode_id =
            apply_resolved_shared_episode(transaction, &candidate.episode, observed_at_ms)?;
        rows.push(serde_json::json!({
            "episode_id": id_text(episode_id.into_bytes()),
            "title": candidate.episode.title,
            "podcast": candidate.episode.podcast_title,
            "published_at_ms": candidate.episode.published_at_milliseconds,
        }));
        episode_ids.push(episode_id);
    }
    let bounded_result = serde_json::to_string(&serde_json::json!({ "episodes": rows }))
        .map_err(|_| StorageError::InvalidActivity)?;
    Ok(crate::StoredLibraryNetworkResult::Catalog {
        episode_ids,
        bounded_result,
    })
}

fn id_text(bytes: [u8; 16]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(32);
    for byte in bytes {
        value.push(char::from(HEX[usize::from(byte >> 4)]));
        value.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    value
}
