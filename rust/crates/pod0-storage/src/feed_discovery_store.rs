use pod0_domain::{
    CommandId, EpisodeId, FeedDiscoveryItemId, FeedDiscoveryOccurrenceId, PodcastId,
    UnixTimestampMilliseconds,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::{
    AppliedFeed, FeedDiscoveryItemRecord, FeedDiscoveryOccurrenceRecord, LibraryStore, StorageError,
};

pub(crate) struct NewFeedDiscoveryItem {
    pub episode_id: EpisodeId,
    pub input_version: String,
    pub published_at_ms: i64,
}

impl LibraryStore {
    pub fn pending_feed_discoveries(
        &self,
        maximum_count: u16,
    ) -> Result<Vec<FeedDiscoveryOccurrenceRecord>, StorageError> {
        let limit = i64::from(maximum_count.clamp(1, 64));
        self.read(|connection| read_pending(connection, limit))
    }
}

pub(crate) fn insert_occurrence(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    podcast_id: PodcastId,
    is_initial_population: bool,
    observed_at_ms: i64,
    items: &[NewFeedDiscoveryItem],
) -> Result<FeedDiscoveryOccurrenceId, StorageError> {
    if items.is_empty() || items.len() > pod0_application::MAX_FEED_DISCOVERY_ITEMS {
        return Err(StorageError::CorruptSchema {
            detail: "feed discovery item count is invalid",
        });
    }
    let occurrence_id = pod0_application::feed_discovery_occurrence_id(command_id);
    transaction
        .execute(
            "INSERT INTO pod0_feed_discovery_occurrences(
                occurrence_id,command_id,podcast_id,state,workflow_schema_version,
                policy_version,is_initial_population,item_count,observed_at_ms,
                created_at_ms,updated_at_ms
             ) VALUES(?1,?2,?3,'pending',?4,?5,?6,?7,?8,?8,?8)",
            params![
                occurrence_id.into_bytes().as_slice(),
                command_id.into_bytes().as_slice(),
                podcast_id.into_bytes().as_slice(),
                pod0_application::FEED_DISCOVERY_WORKFLOW_SCHEMA_VERSION,
                pod0_application::FEED_DISCOVERY_POLICY_VERSION,
                i64::from(is_initial_population),
                i64::try_from(items.len()).map_err(|_| StorageError::CorruptSchema {
                    detail: "feed discovery item count overflows",
                })?,
                observed_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("insert feed discovery occurrence", error))?;
    for item in items {
        let item_id = pod0_application::feed_discovery_item_id(occurrence_id, item.episode_id);
        transaction
            .execute(
                "INSERT INTO pod0_feed_discovery_items(
                    item_id,occurrence_id,episode_id,input_version,published_at_ms
                 ) VALUES(?1,?2,?3,?4,?5)",
                params![
                    item_id.into_bytes().as_slice(),
                    occurrence_id.into_bytes().as_slice(),
                    item.episode_id.into_bytes().as_slice(),
                    item.input_version,
                    item.published_at_ms,
                ],
            )
            .map_err(|error| StorageError::sqlite("insert feed discovery item", error))?;
    }
    Ok(occurrence_id)
}

pub(crate) fn apply_receipt_for_command(
    transaction: &Transaction<'_>,
    command_id: CommandId,
) -> Result<Option<(PodcastId, Option<FeedDiscoveryOccurrenceId>, u32)>, StorageError> {
    let row: Option<(Vec<u8>, Option<Vec<u8>>, i64)> = transaction
        .query_row(
            "SELECT podcast_id,discovery_occurrence_id,inserted_episode_count
             FROM pod0_feed_apply_receipts WHERE command_id=?1",
            [command_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read feed apply receipt", error))?;
    row.map(|(podcast, occurrence, count)| {
        Ok((
            decode_id(podcast, PodcastId::from_bytes)?,
            occurrence
                .map(|value| decode_id(value, FeedDiscoveryOccurrenceId::from_bytes))
                .transpose()?,
            u32::try_from(count).map_err(|_| StorageError::CorruptSchema {
                detail: "feed apply inserted count is malformed",
            })?,
        ))
    })
    .transpose()
}

pub(crate) fn insert_apply_receipt(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    applied: &AppliedFeed,
) -> Result<(), StorageError> {
    let occurrence_id = applied
        .discovery_occurrence_id
        .map(FeedDiscoveryOccurrenceId::into_bytes);
    transaction
        .execute(
            "INSERT INTO pod0_feed_apply_receipts(
                command_id,podcast_id,inserted_episode_count,discovery_occurrence_id
             ) VALUES(?1,?2,?3,?4)",
            params![
                command_id.into_bytes().as_slice(),
                applied.podcast_id.into_bytes().as_slice(),
                applied.inserted_episode_count,
                occurrence_id.as_ref().map(<[u8; 16]>::as_slice),
            ],
        )
        .map_err(|error| StorageError::sqlite("insert feed apply receipt", error))?;
    Ok(())
}

fn read_pending(
    connection: &Connection,
    limit: i64,
) -> Result<Vec<FeedDiscoveryOccurrenceRecord>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT occurrence_id,command_id,podcast_id,workflow_schema_version,
                    policy_version,is_initial_population,item_count,observed_at_ms
             FROM pod0_feed_discovery_occurrences WHERE state='pending'
             ORDER BY observed_at_ms,occurrence_id LIMIT ?1",
        )
        .map_err(|error| StorageError::sqlite("prepare pending feed discoveries", error))?;
    let rows = statement
        .query_map([limit], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("query pending feed discoveries", error))?;
    let mut output = Vec::new();
    for row in rows {
        let (
            occurrence,
            command,
            podcast,
            workflow_schema,
            policy,
            initial_population,
            expected_count,
            observed_at,
        ) = row.map_err(|error| StorageError::sqlite("read pending feed discovery", error))?;
        let occurrence_id = decode_id(occurrence, FeedDiscoveryOccurrenceId::from_bytes)?;
        let items = read_items(connection, occurrence_id)?;
        if i64::try_from(items.len()).ok() != Some(expected_count) {
            return Err(StorageError::CorruptSchema {
                detail: "feed discovery item count does not match",
            });
        }
        output.push(FeedDiscoveryOccurrenceRecord {
            occurrence_id,
            command_id: decode_id(command, CommandId::from_bytes)?,
            podcast_id: decode_id(podcast, PodcastId::from_bytes)?,
            workflow_schema_version: u32::try_from(workflow_schema).map_err(|_| {
                StorageError::CorruptSchema {
                    detail: "feed discovery workflow schema version is malformed",
                }
            })?,
            policy_version: u32::try_from(policy).map_err(|_| StorageError::CorruptSchema {
                detail: "feed discovery policy version is malformed",
            })?,
            is_initial_population: decode_boolean(
                initial_population,
                "feed discovery initial-population flag is malformed",
            )?,
            observed_at: UnixTimestampMilliseconds::new(observed_at),
            items,
        });
    }
    Ok(output)
}

fn read_items(
    connection: &Connection,
    occurrence_id: FeedDiscoveryOccurrenceId,
) -> Result<Vec<FeedDiscoveryItemRecord>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT item_id,episode_id,input_version,published_at_ms
             FROM pod0_feed_discovery_items WHERE occurrence_id=?1
             ORDER BY published_at_ms DESC,episode_id",
        )
        .map_err(|error| StorageError::sqlite("prepare feed discovery items", error))?;
    let rows = statement
        .query_map([occurrence_id.into_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("query feed discovery items", error))?;
    let mut output = Vec::new();
    for row in rows {
        let (item, episode, input_version, published_at) =
            row.map_err(|error| StorageError::sqlite("read feed discovery item", error))?;
        output.push(FeedDiscoveryItemRecord {
            item_id: decode_id(item, FeedDiscoveryItemId::from_bytes)?,
            episode_id: decode_id(episode, EpisodeId::from_bytes)?,
            input_version,
            published_at: UnixTimestampMilliseconds::new(published_at),
        });
    }
    Ok(output)
}

fn decode_id<T>(
    bytes: Vec<u8>,
    constructor: impl FnOnce([u8; 16]) -> T,
) -> Result<T, StorageError> {
    let value: [u8; 16] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
        detail: "feed discovery identity is malformed",
    })?;
    Ok(constructor(value))
}

fn decode_boolean(value: i64, detail: &'static str) -> Result<bool, StorageError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(StorageError::CorruptSchema { detail }),
    }
}
