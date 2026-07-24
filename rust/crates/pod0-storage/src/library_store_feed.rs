use pod0_domain::{CommandId, EpisodeId, EpisodeRecord, PodcastId, PodcastRecord, StateRevision};
use rusqlite::{OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};

use crate::StorageError;
use crate::library_feed_codec;
use crate::library_store::{LibraryStore, command_was_applied, finish_command, source_import_id};
use crate::listening_db_codec::{bool_value, i64_value, podcast_kind};

impl LibraryStore {
    #[allow(clippy::too_many_arguments)]
    pub fn apply_feed(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        mut podcast: PodcastRecord,
        mut episodes: Vec<EpisodeRecord>,
        subscribe: bool,
        entity_tag: Option<String>,
        last_modified: Option<String>,
        observed_at_ms: i64,
    ) -> Result<(StateRevision, PodcastId), StorageError> {
        self.write(|transaction| {
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
                let resolved = resolve_podcast_id(transaction, &podcast)?;
                return Ok((revision, resolved));
            }
            let podcast_id = resolve_podcast_id(transaction, &podcast)?;
            if podcast_id != podcast.podcast_id {
                podcast.podcast_id = podcast_id;
                for episode in &mut episodes {
                    episode.podcast_id = podcast_id;
                    episode.episode_id = episode_id(podcast_id, &episode.publisher_guid);
                }
            }
            podcast.etag = entity_tag.or(podcast.etag);
            podcast.last_modified = last_modified.or(podcast.last_modified);
            upsert_podcast(transaction, &podcast)?;
            for episode in &episodes {
                upsert_episode(transaction, episode)?;
            }
            if subscribe {
                insert_subscription(transaction, podcast_id, observed_at_ms)?;
            }
            let revision =
                finish_command(transaction, command_id, command_fingerprint, observed_at_ms)?;
            Ok((revision, podcast_id))
        })
    }
}

pub(crate) fn resolve_podcast_id(
    transaction: &Transaction<'_>,
    podcast: &PodcastRecord,
) -> Result<PodcastId, StorageError> {
    let Some(feed) = &podcast.feed_identity else {
        return Ok(podcast.podcast_id);
    };
    let stored: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT podcast_id FROM pod0_podcasts WHERE feed_key_v1=?1",
            [&feed.comparison_key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("resolve podcast feed identity", error))?;
    stored.map_or(Ok(podcast.podcast_id), |bytes| {
        let bytes: [u8; 16] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
            detail: "podcast identity is malformed",
        })?;
        Ok(PodcastId::from_bytes(bytes))
    })
}

pub(crate) fn upsert_podcast(
    transaction: &Transaction<'_>,
    podcast: &PodcastRecord,
) -> Result<(), StorageError> {
    let origin = source_import_id(transaction)?;
    let (kind, kind_wire) = podcast_kind(&podcast.kind);
    let categories =
        serde_json::to_string(&podcast.categories).map_err(|_| StorageError::CorruptSchema {
            detail: "podcast categories cannot be encoded",
        })?;
    let feed = podcast.feed_identity.as_ref();
    transaction.execute(
        "INSERT INTO pod0_podcasts(podcast_id,kind_code,kind_wire_code,feed_url,feed_key_v1,\
         title,author,image_url,description,language,categories_json,discovered_at_ms,\
         title_is_placeholder,last_refreshed_at_ms,etag,last_modified,source_import_id) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17) \
         ON CONFLICT(podcast_id) DO UPDATE SET \
         kind_code=excluded.kind_code,kind_wire_code=excluded.kind_wire_code,\
         feed_url=COALESCE(excluded.feed_url,pod0_podcasts.feed_url),\
         feed_key_v1=COALESCE(excluded.feed_key_v1,pod0_podcasts.feed_key_v1),\
         title=CASE WHEN excluded.title='' THEN pod0_podcasts.title ELSE excluded.title END,\
         author=CASE WHEN excluded.author='' THEN pod0_podcasts.author ELSE excluded.author END,\
         image_url=COALESCE(excluded.image_url,pod0_podcasts.image_url),\
         description=CASE WHEN excluded.description='' THEN pod0_podcasts.description ELSE excluded.description END,\
         language=COALESCE(excluded.language,pod0_podcasts.language),\
         categories_json=CASE WHEN excluded.categories_json='[]' THEN pod0_podcasts.categories_json ELSE excluded.categories_json END,\
         title_is_placeholder=excluded.title_is_placeholder,\
         last_refreshed_at_ms=COALESCE(excluded.last_refreshed_at_ms,pod0_podcasts.last_refreshed_at_ms),\
         etag=COALESCE(excluded.etag,pod0_podcasts.etag),\
         last_modified=COALESCE(excluded.last_modified,pod0_podcasts.last_modified),\
         library_visible=1",
        params![podcast.podcast_id.into_bytes().as_slice(), kind, kind_wire,
            feed.map(|value| value.source_url.as_str()),
            feed.map(|value| value.comparison_key.as_str()), podcast.title, podcast.author,
            podcast.image_url, podcast.description, podcast.language, categories,
            podcast.discovered_at.value, bool_value(podcast.title_is_placeholder),
            podcast.last_refreshed_at.map(|value| value.value), podcast.etag,
            podcast.last_modified, origin],
    ).map_err(|error| StorageError::sqlite("upsert podcast feed metadata", error))?;
    Ok(())
}

pub(crate) fn upsert_episode(
    transaction: &Transaction<'_>,
    episode: &EpisodeRecord,
) -> Result<(), StorageError> {
    let origin = source_import_id(transaction)?;
    transaction.execute(
        "INSERT INTO pod0_episodes(episode_id,podcast_id,publisher_guid,title,description,\
         published_at_ms,duration_ms,enclosure_url,enclosure_mime_type,image_url,resume_position_ms,\
         completion_code,is_starred,download_code,transcript_code,legacy_payload,source_import_id) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,0,1,0,1,1,x'7b7d',?11) \
         ON CONFLICT(podcast_id,publisher_guid) DO UPDATE SET \
         title=excluded.title,description=excluded.description,published_at_ms=excluded.published_at_ms,\
         duration_ms=excluded.duration_ms,enclosure_url=excluded.enclosure_url,\
         enclosure_mime_type=excluded.enclosure_mime_type,image_url=excluded.image_url",
        params![episode.episode_id.into_bytes().as_slice(), episode.podcast_id.into_bytes().as_slice(),
            episode.publisher_guid, episode.title, episode.description, episode.published_at.value,
            episode.duration_milliseconds.map(|value| i64_value(value, "duration")).transpose()?,
            episode.enclosure_url, episode.enclosure_mime_type, episode.image_url, origin],
    ).map_err(|error| StorageError::sqlite("upsert feed episode", error))?;
    let actual_id: Vec<u8> = transaction
        .query_row(
            "SELECT episode_id FROM pod0_episodes WHERE podcast_id=?1 AND publisher_guid=?2",
            params![
                episode.podcast_id.into_bytes().as_slice(),
                episode.publisher_guid
            ],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("resolve feed episode identity", error))?;
    let metadata = library_feed_codec::encode(&episode.feed_metadata)?;
    transaction
        .execute(
            "INSERT INTO pod0_episode_feed_metadata(episode_id,publisher_transcript_url,\
         publisher_transcript_media_type,publisher_transcript_format_code,\
         publisher_transcript_format_wire_code,chapters_url,persons_json,sound_bites_json) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(episode_id) DO UPDATE SET \
         publisher_transcript_url=excluded.publisher_transcript_url,\
         publisher_transcript_media_type=excluded.publisher_transcript_media_type,\
         publisher_transcript_format_code=excluded.publisher_transcript_format_code,\
         publisher_transcript_format_wire_code=excluded.publisher_transcript_format_wire_code,\
         chapters_url=excluded.chapters_url,persons_json=excluded.persons_json,\
         sound_bites_json=excluded.sound_bites_json",
            params![
                actual_id,
                metadata.transcript_url,
                metadata.transcript_media_type,
                metadata.transcript_format_code,
                metadata.transcript_format_wire_code,
                metadata.chapters_url,
                metadata.persons_json,
                metadata.sound_bites_json
            ],
        )
        .map_err(|error| StorageError::sqlite("upsert episode feed metadata", error))?;
    Ok(())
}

fn insert_subscription(
    transaction: &Transaction<'_>,
    podcast_id: PodcastId,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let origin = source_import_id(transaction)?;
    transaction
        .execute(
            "INSERT INTO pod0_subscriptions(podcast_id,subscribed_at_ms,auto_download_code,\
         wifi_only,notifications_enabled,source_import_id,transcript_start_policy_code) \
         VALUES(?1,?2,3,1,1,?3,1) \
         ON CONFLICT(podcast_id) DO NOTHING",
            params![podcast_id.into_bytes().as_slice(), observed_at_ms, origin],
        )
        .map_err(|error| StorageError::sqlite("insert podcast subscription", error))?;
    Ok(())
}

pub(crate) fn episode_id(podcast_id: PodcastId, guid: &str) -> EpisodeId {
    let mut hash = Sha256::new();
    hash.update(podcast_id.into_bytes());
    hash.update(guid.as_bytes());
    EpisodeId::from_bytes(hash.finalize()[..16].try_into().expect("digest slice"))
}
