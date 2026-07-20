use std::fs;
use std::path::{Path, PathBuf};

use pod0_domain::ContentDigest;
use rusqlite::{Connection, MAIN_DB, OpenFlags};

use crate::backup::verify_connection;
use crate::legacy_chapter_db_artifacts::artifact_rows;
use crate::legacy_chapter_db_schema::{source_generation, validate_chapter_source_schema};
use crate::transcript_import_digest::{TranscriptImportHash, digest_bytes};
use crate::{LegacyChapterSourceKind, StorageError};

const MAX_EPISODES: usize = 100_000;
const MAX_EPISODE_PAYLOAD_BYTES: usize = 8 * 1_024 * 1_024;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LegacyChapterDatabase {
    pub(crate) source_kind: LegacyChapterSourceKind,
    pub(crate) source_generation: u64,
    pub(crate) source_file_identity: ContentDigest,
    pub(crate) source_database_byte_count: u64,
    pub(crate) source_database_digest: ContentDigest,
    pub(crate) episodes: Vec<LegacyChapterEpisodeRow>,
    pub(crate) artifacts: Vec<LegacyChapterArtifactRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyChapterEpisodeRow {
    pub(crate) subject: String,
    pub(crate) parent: String,
    pub(crate) payload: Vec<u8>,
    pub(crate) payload_digest: ContentDigest,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LegacyChapterArtifactRow {
    pub(crate) row_id: u64,
    pub(crate) kind: String,
    pub(crate) subject: String,
    pub(crate) input_version: String,
    pub(crate) output_version: String,
    pub(crate) content_hash: String,
    pub(crate) location: Option<PathBuf>,
    pub(crate) origin: Option<String>,
    pub(crate) schema_version: i64,
    pub(crate) integrity: String,
    pub(crate) verified_at_seconds: f64,
    pub(crate) legacy_selected: Option<bool>,
    pub(crate) importer_selected: bool,
    pub(crate) row_digest: ContentDigest,
}

pub(crate) fn inspect_legacy_chapter_database(
    source_path: &Path,
) -> Result<LegacyChapterDatabase, StorageError> {
    let temporary = tempfile::tempdir()
        .map_err(|error| StorageError::io("create chapter source snapshot directory", error))?;
    let snapshot_path = temporary.path().join("source.sqlite");
    backup_sqlite(source_path, &snapshot_path)?;
    inspect_chapter_database_snapshot(source_path, &snapshot_path)
}

pub(crate) fn inspect_chapter_database_snapshot(
    source_identity_path: &Path,
    snapshot_path: &Path,
) -> Result<LegacyChapterDatabase, StorageError> {
    let connection = open_read_only(snapshot_path)?;
    verify_connection(&connection)?;
    let source_kind = validate_chapter_source_schema(&connection)?;
    let source_generation = source_generation(&connection)?;
    let episodes = episode_rows(&connection)?;
    let artifacts = artifact_rows(&connection, source_kind, source_identity_path)?;
    let source_file_identity = path_identity(source_identity_path)?;
    let source_database_byte_count = fs::metadata(snapshot_path)
        .map_err(|error| StorageError::io("read chapter database snapshot metadata", error))?
        .len();
    let source_database_digest = database_digest(
        source_kind,
        source_generation,
        source_file_identity,
        &episodes,
        &artifacts,
    );
    Ok(LegacyChapterDatabase {
        source_kind,
        source_generation,
        source_file_identity,
        source_database_byte_count,
        source_database_digest,
        episodes,
        artifacts,
    })
}

pub(crate) fn backup_sqlite(source: &Path, destination: &Path) -> Result<(), StorageError> {
    let connection = open_read_only(source)?;
    connection
        .backup(MAIN_DB, destination, None)
        .map_err(|error| StorageError::sqlite("create chapter database snapshot", error))
}

fn episode_rows(connection: &Connection) -> Result<Vec<LegacyChapterEpisodeRow>, StorageError> {
    let mut statement = connection
        .prepare("SELECT id,subscription_id,payload FROM episodes ORDER BY id")
        .map_err(|error| StorageError::sqlite("prepare chapter episode rows", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read chapter episode rows", error))?;
    let mut result = Vec::new();
    for row in rows {
        let (subject, parent, payload) =
            row.map_err(|error| StorageError::sqlite("decode chapter episode row", error))?;
        if result.len() >= MAX_EPISODES || payload.len() > MAX_EPISODE_PAYLOAD_BYTES {
            return Err(StorageError::ImportLimitExceeded {
                entity: "chapter episode payloads",
            });
        }
        result.push(LegacyChapterEpisodeRow {
            subject,
            parent,
            payload_digest: digest_bytes(&payload),
            payload,
        });
    }
    Ok(result)
}

fn database_digest(
    kind: LegacyChapterSourceKind,
    generation: u64,
    identity: ContentDigest,
    episodes: &[LegacyChapterEpisodeRow],
    artifacts: &[LegacyChapterArtifactRow],
) -> ContentDigest {
    let mut hash = TranscriptImportHash::new(b"pod0.legacy-chapter-database.v1");
    hash.u32(kind.schema_version());
    hash.u64(generation);
    hash.bytes(&identity.into_bytes());
    hash.u64(episodes.len() as u64);
    for row in episodes {
        hash.text(&row.subject);
        hash.text(&row.parent);
        hash.bytes(&row.payload_digest.into_bytes());
    }
    hash.u64(artifacts.len() as u64);
    for row in artifacts {
        hash.bytes(&row.row_digest.into_bytes());
    }
    hash.finish()
}

fn path_identity(path: &Path) -> Result<ContentDigest, StorageError> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| StorageError::io("resolve chapter source path", error))?;
    let text = canonical
        .to_str()
        .ok_or(StorageError::UnsupportedLegacySource)?;
    let mut hash = TranscriptImportHash::new(b"pod0.legacy-chapter-path.v1");
    hash.text(text);
    Ok(hash.finish())
}

fn open_read_only(path: &Path) -> Result<Connection, StorageError> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|error| StorageError::sqlite("open legacy chapter database", error))
}
