use std::collections::BTreeSet;
use std::path::Path;

use pod0_domain::{ContentDigest, EpisodeId, PodcastId};
use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::backup::verify_connection;
use crate::legacy_format::{finite_milliseconds, uuid_bytes};
use crate::legacy_transcript_db_schema::{
    source_generation, source_kind, validate_recorded_schema,
};
use crate::transcript_import_digest::TranscriptImportHash;
use crate::{LegacyTranscriptSourceKind, StorageError};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LegacyTranscriptRow {
    pub(crate) row_id: u64,
    pub(crate) episode_id: EpisodeId,
    pub(crate) podcast_id: PodcastId,
    pub(crate) is_orphan: bool,
    pub(crate) is_selected: bool,
    pub(crate) subject: String,
    pub(crate) input_version: String,
    pub(crate) output_version: String,
    pub(crate) content_hash: String,
    pub(crate) location: Option<String>,
    pub(crate) origin: Option<String>,
    pub(crate) artifact_schema_version: u32,
    pub(crate) integrity: String,
    pub(crate) verified_at_ms: i64,
    pub(crate) row_digest: ContentDigest,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LegacyTranscriptDatabase {
    pub(crate) source_kind: LegacyTranscriptSourceKind,
    pub(crate) source_generation: u64,
    pub(crate) database_digest: ContentDigest,
    pub(crate) rows: Vec<LegacyTranscriptRow>,
}

pub(crate) fn inspect_legacy_transcript_database(
    path: &Path,
) -> Result<LegacyTranscriptDatabase, StorageError> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|error| StorageError::sqlite("open legacy transcript database", error))?;
    connection
        .execute_batch("PRAGMA query_only=ON; PRAGMA foreign_keys=ON; BEGIN DEFERRED;")
        .map_err(|error| StorageError::sqlite("snapshot legacy transcript database", error))?;
    verify_connection(&connection)?;
    let source_kind = source_kind(&connection)?;
    validate_recorded_schema(&connection, source_kind)?;
    let source_generation = source_generation(&connection)?;
    let mut rows = transcript_rows(&connection, source_kind)?;
    attach_parents_and_digests(&connection, &mut rows)?;
    // Winner selection is computed while reading the legacy priority order.
    // Digests and staged rows then use a stable identity order so verification
    // is independent of query-planner ordering for equally-timed history.
    rows.sort_by_key(|row| (row.episode_id, row.row_id));
    let database_digest = database_digest(source_kind, source_generation, &rows);
    Ok(LegacyTranscriptDatabase {
        source_kind,
        source_generation,
        database_digest,
        rows,
    })
}

fn transcript_rows(
    connection: &Connection,
    source_kind: LegacyTranscriptSourceKind,
) -> Result<Vec<LegacyTranscriptRow>, StorageError> {
    let has_artifacts: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='artifacts')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("inspect legacy transcript selections", error))?;
    if !has_artifacts {
        return Ok(Vec::new());
    }
    let selected_value = match source_kind {
        LegacyTranscriptSourceKind::ArtifactSqliteV0 => "NULL",
        LegacyTranscriptSourceKind::ArtifactSqliteV1 => "selected",
    };
    let sql = format!(
        "SELECT id,subject_id,input_version,output_version,content_hash,location,origin,\
         schema_version,integrity,verified_at,{selected_value} FROM artifacts WHERE kind='transcript' \
         ORDER BY subject_id,CASE integrity WHEN 'available' THEN 0 ELSE 1 END,\
         verified_at DESC,id DESC"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| StorageError::sqlite("prepare legacy transcript selections", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, f64>(9)?,
                row.get::<_, Option<i64>>(10)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read legacy transcript selections", error))?;
    let mut identities = BTreeSet::new();
    let mut selected_episodes = BTreeSet::new();
    let mut decoded = Vec::new();
    for (offset, row) in rows.enumerate() {
        let row =
            row.map_err(|error| StorageError::sqlite("decode legacy transcript selection", error))?;
        let index = u32::try_from(offset).map_err(|_| StorageError::ImportLimitExceeded {
            entity: "selected transcripts",
        })?;
        let identity = (row.1.clone(), row.2.clone(), row.3.clone());
        if !identities.insert(identity) {
            return Err(invalid(index, "artifact identity is duplicated"));
        }
        let episode_id = EpisodeId::from_bytes(uuid_bytes(&row.1, "transcript episode", index)?);
        let is_selected = match (source_kind, row.10) {
            (LegacyTranscriptSourceKind::ArtifactSqliteV0, None) => {
                row.8 == "available" && selected_episodes.insert(episode_id)
            }
            (LegacyTranscriptSourceKind::ArtifactSqliteV1, Some(0)) => false,
            (LegacyTranscriptSourceKind::ArtifactSqliteV1, Some(1)) => {
                if !selected_episodes.insert(episode_id) {
                    return Err(invalid(index, "multiple transcript artifacts are selected"));
                }
                true
            }
            _ => return Err(invalid(index, "artifact selection flag is invalid")),
        };
        if is_selected && row.8 != "available" {
            return Err(invalid(index, "selected transcript is not available"));
        }
        let schema_version =
            u32::try_from(row.7).map_err(|_| invalid(index, "artifact schema is invalid"))?;
        if schema_version > 1 {
            return Err(StorageError::NewerLegacyTranscriptSchema {
                stored: schema_version,
                supported: 1,
            });
        }
        if schema_version != 1 {
            return Err(invalid(index, "artifact schema is unsupported"));
        }
        decoded.push(LegacyTranscriptRow {
            row_id: u64::try_from(row.0)
                .map_err(|_| invalid(index, "artifact row identity is invalid"))?,
            episode_id,
            podcast_id: PodcastId::from_parts(0, 0),
            is_orphan: false,
            is_selected,
            subject: row.1,
            input_version: bounded(row.2, 1_024, index, "input version")?,
            output_version: bounded(row.3, 1_024, index, "output version")?,
            content_hash: row.4,
            location: row.5,
            origin: optional_bounded(row.6, 128, index, "origin")?,
            artifact_schema_version: schema_version,
            integrity: row.8,
            verified_at_ms: finite_milliseconds(row.9, "transcript selection", index)?,
            row_digest: ContentDigest::default(),
        });
    }
    Ok(decoded)
}

fn attach_parents_and_digests(
    connection: &Connection,
    rows: &mut [LegacyTranscriptRow],
) -> Result<(), StorageError> {
    for (offset, row) in rows.iter_mut().enumerate() {
        let parent: Option<String> = connection
            .query_row(
                "SELECT subscription_id FROM episodes WHERE id=?1",
                [&row.subject],
                |value| value.get(0),
            )
            .optional()
            .map_err(|error| {
                StorageError::sqlite("read legacy transcript episode parent", error)
            })?;
        if let Some(parent) = parent {
            row.podcast_id = PodcastId::from_bytes(uuid_bytes(
                &parent,
                "transcript podcast",
                u32::try_from(offset).unwrap_or(u32::MAX),
            )?);
        } else {
            row.podcast_id = orphan_transcript_podcast_id();
            row.is_orphan = true;
        }
        row.row_digest = row_digest(row);
    }
    Ok(())
}

fn row_digest(row: &LegacyTranscriptRow) -> ContentDigest {
    let mut hash = TranscriptImportHash::new(b"pod0.legacy-transcript-row.v1");
    hash.u64(row.row_id);
    hash.bytes(&row.episode_id.into_bytes());
    hash.bytes(&row.podcast_id.into_bytes());
    hash.u32(u32::from(row.is_orphan));
    hash.u32(u32::from(row.is_selected));
    hash.text(&row.input_version);
    hash.text(&row.output_version);
    hash.text(&row.content_hash);
    hash.optional_text(row.location.as_deref());
    hash.optional_text(row.origin.as_deref());
    hash.u32(row.artifact_schema_version);
    hash.text(&row.integrity);
    hash.i64(row.verified_at_ms);
    hash.finish()
}

pub(crate) const fn orphan_transcript_podcast_id() -> PodcastId {
    crate::retained_orphan_parent::retained_orphan_podcast_id()
}

fn database_digest(
    kind: LegacyTranscriptSourceKind,
    generation: u64,
    rows: &[LegacyTranscriptRow],
) -> ContentDigest {
    let mut hash = TranscriptImportHash::new(b"pod0.legacy-transcript-database.v1");
    hash.u32(kind.schema_version());
    hash.u64(generation);
    hash.u64(rows.len() as u64);
    for row in rows {
        hash.bytes(&row.row_digest.into_bytes());
    }
    hash.finish()
}

fn bounded(
    value: String,
    limit: usize,
    index: u32,
    field: &'static str,
) -> Result<String, StorageError> {
    if value.is_empty() || value.len() > limit {
        Err(invalid(index, field))
    } else {
        Ok(value)
    }
}

fn optional_bounded(
    value: Option<String>,
    limit: usize,
    index: u32,
    field: &'static str,
) -> Result<Option<String>, StorageError> {
    value
        .map(|value| bounded(value, limit, index, field))
        .transpose()
}

fn invalid(index: u32, detail: &'static str) -> StorageError {
    StorageError::InvalidLegacyRecord {
        entity: "transcript selection",
        index,
        detail,
    }
}
