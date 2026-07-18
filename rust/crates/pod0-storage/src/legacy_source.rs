use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};

use crate::import_model::{InspectedLegacySource, LegacyImportPlan, LegacySourceKind};
use crate::legacy_format::{RawAppState, RawEpisode, timestamp_milliseconds, uuid_bytes};
use crate::legacy_transform::transform_source;
use crate::{StorageError, backup::verify_connection};

const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";
const MAX_METADATA_BYTES: u64 = 64 * 1024 * 1024;
const MAX_JSON_BYTES: u64 = 512 * 1024 * 1024;
const MAX_EPISODE_BYTES: usize = 8 * 1024 * 1024;
const MAX_EPISODES: usize = 100_000;

pub fn inspect_legacy_listening_source(path: &Path) -> Result<LegacyImportPlan, StorageError> {
    inspect_source(path).map(|source| source.plan)
}

pub(crate) fn inspect_source(path: &Path) -> Result<InspectedLegacySource, StorageError> {
    let mut file =
        File::open(path).map_err(|error| StorageError::io("open legacy source", error))?;
    let mut header = [0_u8; 16];
    let read = file
        .read(&mut header)
        .map_err(|error| StorageError::io("read legacy source header", error))?;
    file.rewind()
        .map_err(|error| StorageError::io("rewind legacy source", error))?;
    if read == SQLITE_HEADER.len() && &header == SQLITE_HEADER {
        inspect_sqlite(path)
    } else {
        inspect_json(file)
    }
}

fn inspect_json(mut file: File) -> Result<InspectedLegacySource, StorageError> {
    let byte_count = file
        .metadata()
        .map_err(|error| StorageError::io("read legacy JSON metadata", error))?
        .len();
    if byte_count > MAX_JSON_BYTES {
        return Err(StorageError::ImportLimitExceeded {
            entity: "legacy JSON bytes",
        });
    }
    let mut bytes = Vec::with_capacity(usize::try_from(byte_count).unwrap_or(0));
    file.read_to_end(&mut bytes)
        .map_err(|error| StorageError::io("read legacy JSON", error))?;
    let raw: RawAppState =
        serde_json::from_slice(&bytes).map_err(|_| StorageError::InvalidLegacyRecord {
            entity: "metadata",
            index: 0,
            detail: "legacy AppState is not recognized JSON",
        })?;
    let hash = digest([b"pod0-legacy-json-v1".as_slice(), bytes.as_slice()]);
    transform_source(raw, None, LegacySourceKind::LegacyJson, hash)
}

fn inspect_sqlite(path: &Path) -> Result<InspectedLegacySource, StorageError> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|error| StorageError::sqlite("open Swift SQLite source", error))?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| StorageError::sqlite("configure Swift SQLite source", error))?;
    connection
        .execute_batch("PRAGMA query_only=ON; PRAGMA foreign_keys=ON;")
        .map_err(|error| StorageError::sqlite("configure read-only Swift source", error))?;
    verify_connection(&connection)?;
    require_columns(&connection, "persistence_metadata", &["key", "value"])?;
    require_columns(
        &connection,
        "episodes",
        &[
            "guid",
            "id",
            "payload",
            "pub_date",
            "sort_order",
            "subscription_id",
        ],
    )?;
    let metadata: Vec<u8> = connection
        .query_row(
            "SELECT value FROM persistence_metadata WHERE key='app_state'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read Swift AppState metadata", error))?;
    if metadata.len() as u64 > MAX_METADATA_BYTES {
        return Err(StorageError::ImportLimitExceeded {
            entity: "metadata bytes",
        });
    }
    let generation_text: String = connection
        .query_row(
            "SELECT CAST(value AS TEXT) FROM persistence_metadata WHERE key='generation'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read Swift source generation", error))?;
    let generation =
        generation_text
            .parse::<u64>()
            .map_err(|_| StorageError::InvalidLegacyRecord {
                entity: "metadata",
                index: 0,
                detail: "persistence generation is invalid",
            })?;
    let payloads = read_episode_payloads(&connection)?;
    let mut raw: RawAppState =
        serde_json::from_slice(&metadata).map_err(|_| StorageError::InvalidLegacyRecord {
            entity: "metadata",
            index: 0,
            detail: "Swift AppState metadata is not recognized JSON",
        })?;
    if raw.generation != 0 && raw.generation != generation {
        return Err(StorageError::InvalidLegacyRecord {
            entity: "metadata",
            index: 0,
            detail: "metadata and SQLite generations differ",
        });
    }
    raw.generation = generation;
    let hash = sqlite_digest(generation, &metadata, &payloads);
    transform_source(raw, Some(payloads), LegacySourceKind::SwiftSqlite, hash)
}

fn read_episode_payloads(connection: &Connection) -> Result<Vec<Vec<u8>>, StorageError> {
    let count: u32 = connection
        .query_row("SELECT COUNT(*) FROM episodes", [], |row| row.get(0))
        .map_err(|error| StorageError::sqlite("count Swift episodes", error))?;
    let count = usize::try_from(count)
        .map_err(|_| StorageError::ImportLimitExceeded { entity: "episodes" })?;
    if count > MAX_EPISODES {
        return Err(StorageError::ImportLimitExceeded { entity: "episodes" });
    }
    let mut statement = connection
        .prepare(
            "SELECT id,subscription_id,guid,pub_date,sort_order,payload \
             FROM episodes ORDER BY sort_order ASC",
        )
        .map_err(|error| StorageError::sqlite("read Swift episodes", error))?;
    let mut rows = statement
        .query([])
        .map_err(|error| StorageError::sqlite("query Swift episodes", error))?;
    let mut payloads = Vec::with_capacity(count);
    while let Some(row) = rows
        .next()
        .map_err(|error| StorageError::sqlite("advance Swift episode row", error))?
    {
        let index = u32::try_from(payloads.len())
            .map_err(|_| StorageError::ImportLimitExceeded { entity: "episodes" })?;
        let id: String = row
            .get(0)
            .map_err(|error| StorageError::sqlite("read Swift episode ID", error))?;
        let parent: String = row
            .get(1)
            .map_err(|error| StorageError::sqlite("read Swift episode parent", error))?;
        let guid: String = row
            .get(2)
            .map_err(|error| StorageError::sqlite("read Swift episode GUID", error))?;
        let published_seconds: f64 = row
            .get(3)
            .map_err(|error| StorageError::sqlite("read Swift episode date", error))?;
        let sort_order: i64 = row
            .get(4)
            .map_err(|error| StorageError::sqlite("read Swift episode order", error))?;
        let payload: Vec<u8> = row
            .get(5)
            .map_err(|error| StorageError::sqlite("read Swift episode payload", error))?;
        if payload.len() > MAX_EPISODE_BYTES || sort_order != i64::from(index) {
            return Err(invalid_episode_row(
                index,
                "episode order or payload size is invalid",
            ));
        }
        validate_episode_row(&payload, &id, &parent, &guid, published_seconds, index)?;
        payloads.push(payload);
    }
    Ok(payloads)
}

fn validate_episode_row(
    payload: &[u8],
    stored_id: &str,
    stored_parent: &str,
    stored_guid: &str,
    stored_published_seconds: f64,
    index: u32,
) -> Result<(), StorageError> {
    let decoded: RawEpisode = serde_json::from_slice(payload)
        .map_err(|_| invalid_episode_row(index, "episode payload is not recognized JSON"))?;
    let decoded_parent = decoded
        .podcast_id
        .as_ref()
        .or(decoded.legacy_subscription_id.as_ref())
        .ok_or_else(|| invalid_episode_row(index, "episode parent is missing"))?;
    let row_matches = uuid_bytes(&decoded.id, "episode", index)?
        == uuid_bytes(stored_id, "episode", index)?
        && uuid_bytes(decoded_parent, "episode", index)?
            == uuid_bytes(stored_parent, "episode", index)?
        && decoded.guid == stored_guid;
    let payload_milliseconds =
        timestamp_milliseconds(decoded.published_at.as_ref(), "episode", index)?;
    let date_matches =
        ((stored_published_seconds * 1_000.0).round() - payload_milliseconds as f64).abs() <= 1.0;
    if row_matches && date_matches {
        Ok(())
    } else {
        Err(invalid_episode_row(
            index,
            "episode index columns disagree with its payload",
        ))
    }
}

fn require_columns(
    connection: &Connection,
    table: &'static str,
    required: &[&str],
) -> Result<(), StorageError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| StorageError::sqlite("inspect Swift source table", error))?;
    let actual = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| StorageError::sqlite("inspect Swift source columns", error))?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(|error| StorageError::sqlite("read Swift source columns", error))?;
    if required.iter().all(|column| actual.contains(*column)) {
        Ok(())
    } else {
        Err(StorageError::UnsupportedLegacySource)
    }
}

fn sqlite_digest(generation: u64, metadata: &[u8], payloads: &[Vec<u8>]) -> String {
    let mut hasher = Sha256::new();
    hash_part(&mut hasher, b"pod0-swift-sqlite-v1");
    hash_part(&mut hasher, &generation.to_be_bytes());
    hash_part(&mut hasher, metadata);
    for payload in payloads {
        hash_part(&mut hasher, payload);
    }
    hex(&hasher.finalize())
}

fn digest<const N: usize>(parts: [&[u8]; N]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hash_part(&mut hasher, part);
    }
    hex(&hasher.finalize())
}

fn hash_part(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(value);
}

fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn invalid_episode_row(index: u32, detail: &'static str) -> StorageError {
    StorageError::InvalidLegacyRecord {
        entity: "episode",
        index,
        detail,
    }
}
