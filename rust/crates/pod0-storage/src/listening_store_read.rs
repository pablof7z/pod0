use pod0_domain::{
    AutoDownloadPolicy, EpisodeId, FeedIdentityV1, ListeningDomainSnapshot,
    ListeningPlaybackPolicy, PlaybackRatePermille, PlaybackSegment, PodcastId, PodcastRecord,
    PodcastSubscriptionRecord, QueueEntry, QueueEntryId, StateRevision, UnixTimestampMilliseconds,
};
use rusqlite::{Connection, OptionalExtension, Row};

use crate::import_model::{LegacyBackupEvidence, ListeningImportReport};
use crate::listening_db_codec::{
    corrupt, decode_auto_download, decode_podcast_kind, decode_sleep,
    decode_transcript_start_policy,
};
use crate::listening_store_read_episode::read_episodes;
use crate::{LegacyImportPlan, LegacySourceKind, StorageError};

pub(crate) fn read_snapshot(
    connection: &Connection,
) -> Result<ListeningDomainSnapshot, StorageError> {
    Ok(ListeningDomainSnapshot {
        podcasts: read_podcasts(connection)?,
        subscriptions: read_subscriptions(connection)?,
        episodes: read_episodes(connection)?,
        playback: read_playback(connection)?,
    })
}

pub(crate) fn stored_import_report(
    connection: &Connection,
    import_id: pod0_domain::CommandId,
    backup: Option<&LegacyBackupEvidence>,
) -> Result<Option<ListeningImportReport>, StorageError> {
    let row = connection.query_row(
        "SELECT source_kind,source_hash,source_generation,podcast_count,subscription_count,\
         episode_count,backup_byte_count,target_revision FROM pod0_listening_imports WHERE import_id=?1",
        [import_id.into_bytes().as_slice()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?)),
    ).optional().map_err(|error| StorageError::sqlite("read listening import", error))?;
    let Some((kind, hash, generation, podcasts, subscriptions, episodes, bytes, revision)) = row
    else {
        return Ok(None);
    };
    let source_kind = LegacySourceKind::from_code(kind).ok_or_else(|| corrupt("source kind"))?;
    let plan = LegacyImportPlan {
        source_kind,
        source_hash: hash,
        source_generation: unsigned(generation, "source generation")?,
        podcast_count: count(podcasts, "podcast count")?,
        subscription_count: count(subscriptions, "subscription count")?,
        episode_count: count(episodes, "episode count")?,
    };
    let stored_backup = LegacyBackupEvidence {
        source_kind,
        source_hash: plan.source_hash.clone(),
        source_generation: plan.source_generation,
        byte_count: unsigned(bytes, "backup byte count")?,
        reused_existing: true,
    };
    if let Some(expected) = backup {
        let same = expected.source_kind == stored_backup.source_kind
            && expected.source_hash == stored_backup.source_hash
            && expected.source_generation == stored_backup.source_generation
            && expected.byte_count == stored_backup.byte_count;
        if !same {
            return Err(StorageError::ImportConflict);
        }
    }
    Ok(Some(ListeningImportReport {
        import_id,
        plan,
        target_revision: unsigned(revision, "target revision")?,
        backup: stored_backup,
        staged: cutover_is_staged(connection)?,
        reused_existing: true,
    }))
}

fn read_podcasts(connection: &Connection) -> Result<Vec<PodcastRecord>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT podcast_id,kind_code,kind_wire_code,feed_url,feed_key_v1,title,author,image_url,\
         description,language,categories_json,discovered_at_ms,title_is_placeholder,\
         last_refreshed_at_ms,etag,last_modified FROM pod0_podcasts \
         WHERE library_visible=1 ORDER BY rowid",
    ).map_err(|error| StorageError::sqlite("prepare podcast projection", error))?;
    collect_rows(&mut statement, "read podcast projection", |row| {
        let categories: String = row.get(10)?;
        Ok(PodcastRecord {
            podcast_id: PodcastId::from_bytes(id(row, 0)?),
            kind: decode_podcast_kind(row.get(1)?, row.get(2)?)?,
            feed_identity: feed(row.get(3)?, row.get(4)?)?,
            title: row.get(5)?,
            author: row.get(6)?,
            image_url: row.get(7)?,
            description: row.get(8)?,
            language: row.get(9)?,
            categories: serde_json::from_str(&categories)
                .map_err(|_| corrupt("podcast categories"))?,
            discovered_at: UnixTimestampMilliseconds::new(row.get(11)?),
            title_is_placeholder: boolean(row.get(12)?)?,
            last_refreshed_at: row
                .get::<_, Option<i64>>(13)?
                .map(UnixTimestampMilliseconds::new),
            etag: row.get(14)?,
            last_modified: row.get(15)?,
        })
    })
}

fn read_subscriptions(
    connection: &Connection,
) -> Result<Vec<PodcastSubscriptionRecord>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT podcast_id,subscribed_at_ms,auto_download_code,auto_download_wire_code,\
         auto_download_latest_count,wifi_only,notifications_enabled,default_playback_rate_permille,\
         transcript_start_policy_code,transcript_start_policy_wire_code \
         FROM pod0_subscriptions ORDER BY rowid",
        )
        .map_err(|error| StorageError::sqlite("prepare subscription projection", error))?;
    collect_rows(&mut statement, "read subscription projection", |row| {
        Ok(PodcastSubscriptionRecord {
            podcast_id: PodcastId::from_bytes(id(row, 0)?),
            subscribed_at: UnixTimestampMilliseconds::new(row.get(1)?),
            auto_download: AutoDownloadPolicy {
                mode: decode_auto_download(row.get(2)?, row.get(3)?, row.get(4)?)?,
                wifi_only: boolean(row.get(5)?)?,
            },
            notifications_enabled: boolean(row.get(6)?)?,
            default_playback_rate: row
                .get::<_, Option<u16>>(7)?
                .map(|value| PlaybackRatePermille { value }),
            transcript_start_policy: decode_transcript_start_policy(row.get(8)?, row.get(9)?)?,
        })
    })
}

fn read_playback(connection: &Connection) -> Result<ListeningPlaybackPolicy, StorageError> {
    let playback = connection
        .query_row(
            "SELECT active_episode_id,active_segment_start_ms,active_segment_end_ms,\
         active_segment_label,playback_rate_permille,sleep_mode_code,sleep_duration_ms,\
         sleep_wire_code,auto_mark_played_at_natural_end,auto_play_next,state_revision \
         FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| {
                Ok(StoredPlayback {
                    active_episode_id: row.get(0)?,
                    active_segment_start: row.get(1)?,
                    active_segment_end: row.get(2)?,
                    active_label: row.get(3)?,
                    rate: row.get(4)?,
                    sleep_code: row.get(5)?,
                    sleep_duration: row.get(6)?,
                    sleep_wire: row.get(7)?,
                    auto_mark_played: row.get(8)?,
                    auto_play_next: row.get(9)?,
                    revision: row.get(10)?,
                })
            },
        )
        .map_err(|error| StorageError::sqlite("read playback projection", error))?;
    Ok(ListeningPlaybackPolicy {
        active_episode_id: playback
            .active_episode_id
            .map(|bytes: Vec<u8>| id_from_bytes(bytes).map(EpisodeId::from_bytes))
            .transpose()?,
        active_segment: decode_segment(playback.active_segment_start, playback.active_segment_end)?,
        active_label: playback.active_label,
        queue: read_queue(connection)?,
        rate: PlaybackRatePermille {
            value: playback.rate,
        },
        sleep_mode: decode_sleep(
            playback.sleep_code,
            playback.sleep_duration,
            playback.sleep_wire,
        )?,
        auto_mark_played_at_natural_end: boolean(playback.auto_mark_played)?,
        auto_play_next: boolean(playback.auto_play_next)?,
        revision: StateRevision::new(unsigned(playback.revision, "playback revision")?),
    })
}

struct StoredPlayback {
    active_episode_id: Option<Vec<u8>>,
    active_segment_start: Option<i64>,
    active_segment_end: Option<i64>,
    active_label: Option<String>,
    rate: u16,
    sleep_code: i64,
    sleep_duration: Option<i64>,
    sleep_wire: Option<i64>,
    auto_mark_played: i64,
    auto_play_next: i64,
    revision: i64,
}

fn decode_segment(
    start: Option<i64>,
    end: Option<i64>,
) -> Result<Option<PlaybackSegment>, StorageError> {
    let start = optional_unsigned(start, "active segment start")?;
    let end = optional_unsigned(end, "active segment end")?;
    Ok(if start.is_some() || end.is_some() {
        Some(PlaybackSegment {
            start_position_milliseconds: start,
            end_position_milliseconds: end,
        })
    } else {
        None
    })
}

fn read_queue(connection: &Connection) -> Result<Vec<QueueEntry>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT queue_entry_id,episode_id,segment_start_ms,segment_end_ms,label \
         FROM pod0_queue_entries ORDER BY sort_order",
        )
        .map_err(|error| StorageError::sqlite("prepare queue projection", error))?;
    collect_rows(&mut statement, "read queue projection", |row| {
        let start = optional_unsigned(row.get(2)?, "segment start")?;
        let end = optional_unsigned(row.get(3)?, "segment end")?;
        Ok(QueueEntry {
            queue_entry_id: QueueEntryId::from_bytes(id(row, 0)?),
            episode_id: EpisodeId::from_bytes(id(row, 1)?),
            segment: if start.is_some() || end.is_some() {
                Some(PlaybackSegment {
                    start_position_milliseconds: start,
                    end_position_milliseconds: end,
                })
            } else {
                None
            },
            label: row.get(4)?,
        })
    })
}

fn collect_rows<T, F>(
    statement: &mut rusqlite::Statement<'_>,
    operation: &'static str,
    mut decode: F,
) -> Result<Vec<T>, StorageError>
where
    F: FnMut(&Row<'_>) -> Result<T, StorageError>,
{
    let mut rows = statement
        .query([])
        .map_err(|error| StorageError::sqlite(operation, error))?;
    let mut values = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|error| StorageError::sqlite(operation, error))?
    {
        values.push(decode(row)?);
    }
    Ok(values)
}

fn feed(url: Option<String>, key: Option<String>) -> Result<Option<FeedIdentityV1>, StorageError> {
    match (url, key) {
        (None, None) => Ok(None),
        (Some(source_url), Some(comparison_key)) => Ok(Some(FeedIdentityV1 {
            source_url,
            comparison_key,
        })),
        _ => Err(corrupt("feed identity")),
    }
}
fn id(row: &Row<'_>, index: usize) -> Result<[u8; 16], StorageError> {
    id_from_bytes(row.get(index)?)
}
fn id_from_bytes(value: Vec<u8>) -> Result<[u8; 16], StorageError> {
    value.try_into().map_err(|_| corrupt("stored ID length"))
}
fn unsigned(value: i64, detail: &'static str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| corrupt(detail))
}
fn optional_unsigned(
    value: Option<i64>,
    detail: &'static str,
) -> Result<Option<u64>, StorageError> {
    value.map(|value| unsigned(value, detail)).transpose()
}
fn count(value: i64, detail: &'static str) -> Result<u32, StorageError> {
    u32::try_from(value).map_err(|_| corrupt(detail))
}
fn boolean(value: i64) -> Result<bool, StorageError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(corrupt("boolean")),
    }
}
fn cutover_is_staged(connection: &Connection) -> Result<bool, StorageError> {
    connection
        .query_row(
            "SELECT state='staged' FROM pod0_domain_cutovers WHERE domain='listening'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read listening cutover state", error))
}
