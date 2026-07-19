use pod0_domain::{
    CommandId, EpisodeId, FeedIdentityV1, PodcastId, PodcastKind, PodcastRecord, StateRevision,
    UnixTimestampMilliseconds,
};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::StorageError;
use crate::library_store::{LibraryStore, command_was_applied, finish_command, source_import_id};
use crate::library_store_feed::{episode_id, resolve_podcast_id, upsert_podcast};
use crate::listening_db_codec::i64_value;

impl LibraryStore {
    pub fn upsert_synthetic_podcast(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        podcast: PodcastRecord,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        self.write(|transaction| {
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
                return Ok(revision);
            }
            if podcast.kind != PodcastKind::Synthetic || podcast.feed_identity.is_some() {
                return Err(StorageError::CommandConflict);
            }
            let existing_kind: Option<i64> = transaction
                .query_row(
                    "SELECT kind_code FROM pod0_podcasts WHERE podcast_id=?1",
                    [podcast.podcast_id.into_bytes().as_slice()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| StorageError::sqlite("find synthetic podcast", error))?;
            if existing_kind.is_some_and(|kind| kind != 2) {
                return Err(StorageError::CommandConflict);
            }
            let origin = source_import_id(transaction)?;
            let categories = serde_json::to_string(&podcast.categories).map_err(|_| {
                StorageError::CorruptSchema {
                    detail: "synthetic podcast categories cannot be encoded",
                }
            })?;
            transaction.execute(
                "INSERT INTO pod0_podcasts(podcast_id,kind_code,feed_url,feed_key_v1,title,author,\
                 image_url,description,language,categories_json,discovered_at_ms,\
                 title_is_placeholder,source_import_id) \
                 VALUES(?1,2,NULL,NULL,?2,?3,?4,?5,?6,?7,?8,0,?9) \
                 ON CONFLICT(podcast_id) DO UPDATE SET title=excluded.title,author=excluded.author,\
                 image_url=excluded.image_url,description=excluded.description,\
                 language=excluded.language,categories_json=excluded.categories_json,\
                 title_is_placeholder=0,library_visible=1",
                params![
                    podcast.podcast_id.into_bytes().as_slice(),
                    podcast.title,
                    podcast.author,
                    podcast.image_url,
                    podcast.description,
                    podcast.language,
                    categories,
                    podcast.discovered_at.value,
                    origin,
                ],
            )
            .map_err(|error| StorageError::sqlite("upsert synthetic podcast", error))?;
            finish_command(transaction, command_id, command_fingerprint, observed_at_ms)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_external_episode(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        requested_podcast_id: PodcastId,
        feed_identity: Option<FeedIdentityV1>,
        podcast_title: &str,
        audio_url: &str,
        title: &str,
        description: &str,
        published_at_ms: i64,
        enclosure_mime_type: Option<&str>,
        image_url: Option<&str>,
        duration_milliseconds: Option<u64>,
        observed_at_ms: i64,
    ) -> Result<(StateRevision, PodcastId, EpisodeId), StorageError> {
        self.write(|transaction| {
            let podcast_id = ensure_external_parent(
                transaction,
                requested_podcast_id,
                feed_identity,
                podcast_title,
                observed_at_ms,
            )?;
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
                let episode_id = find_episode_id(transaction, podcast_id, audio_url)?;
                return Ok((revision, podcast_id, episode_id));
            }
            let origin = source_import_id(transaction)?;
            let proposed_episode_id = episode_id(podcast_id, audio_url);
            transaction.execute(
                "INSERT INTO pod0_episodes(episode_id,podcast_id,publisher_guid,title,description,\
                 published_at_ms,duration_ms,enclosure_url,enclosure_mime_type,image_url,\
                 resume_position_ms,completion_code,is_starred,download_code,transcript_code,\
                 legacy_payload,source_import_id) \
                VALUES(?1,?2,?3,?4,?5,?6,?7,?3,?8,?9,0,1,0,1,1,x'7b7d',?10) \
                 ON CONFLICT(podcast_id,publisher_guid) DO UPDATE SET \
                 title=CASE WHEN excluded.title='' THEN pod0_episodes.title ELSE excluded.title END,\
                 description=excluded.description,\
                 duration_ms=COALESCE(excluded.duration_ms,pod0_episodes.duration_ms),\
                 enclosure_mime_type=COALESCE(excluded.enclosure_mime_type,pod0_episodes.enclosure_mime_type),\
                 image_url=COALESCE(excluded.image_url,pod0_episodes.image_url),\
                 enclosure_url=excluded.enclosure_url",
                params![
                    proposed_episode_id.into_bytes().as_slice(),
                    podcast_id.into_bytes().as_slice(),
                    audio_url,
                    title,
                    description,
                    published_at_ms,
                    duration_milliseconds
                        .map(|value| i64_value(value, "external episode duration"))
                        .transpose()?,
                    enclosure_mime_type,
                    image_url,
                    origin,
                ],
            ).map_err(|error| StorageError::sqlite("upsert external episode", error))?;
            let actual_episode_id = find_episode_id(transaction, podcast_id, audio_url)?;
            transaction.execute(
                "INSERT INTO pod0_episode_feed_metadata(episode_id,persons_json,sound_bites_json) \
                 VALUES(?1,'[]','[]') ON CONFLICT(episode_id) DO NOTHING",
                [actual_episode_id.into_bytes().as_slice()],
            ).map_err(|error| StorageError::sqlite("initialize external episode metadata", error))?;
            let revision =
                finish_command(transaction, command_id, command_fingerprint, observed_at_ms)?;
            Ok((revision, podcast_id, actual_episode_id))
        })
    }
}

fn ensure_external_parent(
    transaction: &Transaction<'_>,
    requested_id: PodcastId,
    feed_identity: Option<FeedIdentityV1>,
    title: &str,
    observed_at_ms: i64,
) -> Result<PodcastId, StorageError> {
    let requested_exists: Option<i64> = transaction
        .query_row(
            "SELECT 1 FROM pod0_podcasts WHERE podcast_id=?1",
            [requested_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("find external episode parent", error))?;
    if requested_exists.is_some() {
        transaction
            .execute(
                "UPDATE pod0_podcasts SET library_visible=1 WHERE podcast_id=?1",
                [requested_id.into_bytes().as_slice()],
            )
            .map_err(|error| StorageError::sqlite("restore external episode parent", error))?;
        return Ok(requested_id);
    }

    let kind = if feed_identity.is_some() {
        PodcastKind::Rss
    } else {
        PodcastKind::Synthetic
    };
    let placeholder = PodcastRecord {
        podcast_id: requested_id,
        kind,
        feed_identity,
        title: title.to_owned(),
        author: String::new(),
        image_url: None,
        description: String::new(),
        language: None,
        categories: Vec::new(),
        discovered_at: UnixTimestampMilliseconds::new(observed_at_ms),
        title_is_placeholder: matches!(kind, PodcastKind::Rss),
        last_refreshed_at: None,
        etag: None,
        last_modified: None,
    };
    let resolved = resolve_podcast_id(transaction, &placeholder)?;
    if resolved == requested_id {
        upsert_podcast(transaction, &placeholder)?;
    } else {
        transaction
            .execute(
                "UPDATE pod0_podcasts SET library_visible=1 WHERE podcast_id=?1",
                [resolved.into_bytes().as_slice()],
            )
            .map_err(|error| StorageError::sqlite("restore resolved episode parent", error))?;
    }
    Ok(resolved)
}

fn find_episode_id(
    transaction: &Transaction<'_>,
    podcast_id: PodcastId,
    guid: &str,
) -> Result<EpisodeId, StorageError> {
    let bytes: Vec<u8> = transaction
        .query_row(
            "SELECT episode_id FROM pod0_episodes WHERE podcast_id=?1 AND publisher_guid=?2",
            params![podcast_id.into_bytes().as_slice(), guid],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("resolve external episode identity", error))?;
    let bytes: [u8; 16] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
        detail: "external episode identity is malformed",
    })?;
    Ok(EpisodeId::from_bytes(bytes))
}
