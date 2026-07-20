use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pod0_domain::ContentDigest;
use rusqlite::{Connection, OptionalExtension};

use crate::legacy_chapter_db::LegacyChapterArtifactRow;
use crate::transcript_import_digest::TranscriptImportHash;
use crate::{LegacyChapterSourceKind, StorageError};

const MAX_ARTIFACT_ROWS: usize = 100_000;

pub(crate) fn artifact_rows(
    connection: &Connection,
    source_kind: LegacyChapterSourceKind,
    source_identity_path: &Path,
) -> Result<Vec<LegacyChapterArtifactRow>, StorageError> {
    if !table_exists(connection, "artifacts")? {
        return Ok(Vec::new());
    }
    let rollback_format: Option<i64> = if table_exists(connection, "pod0_chapter_rollback")? {
        connection
            .query_row(
                "SELECT format_version FROM pod0_chapter_rollback WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| StorageError::sqlite("read chapter rollback marker", error))?
    } else {
        None
    };
    let selected = match source_kind {
        LegacyChapterSourceKind::ArtifactSqliteV0 => "NULL",
        LegacyChapterSourceKind::ArtifactSqliteV1 => "selected",
    };
    let sql = format!(
        "SELECT id,kind,subject_id,input_version,output_version,content_hash,location,origin,\
         schema_version,integrity,verified_at,{selected} FROM artifacts \
         WHERE kind IN ('chapters','adSegments') ORDER BY kind,subject_id,\
         CASE integrity WHEN 'available' THEN 0 ELSE 1 END,verified_at DESC,id DESC"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| StorageError::sqlite("prepare legacy chapter rows", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, f64>(10)?,
                row.get::<_, Option<i64>>(11)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read legacy chapter rows", error))?;
    let mut selected_subjects = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let mut decoded = Vec::new();
    for row in rows {
        let row = row.map_err(|error| StorageError::sqlite("decode legacy chapter row", error))?;
        if decoded.len() >= MAX_ARTIFACT_ROWS {
            return Err(StorageError::ImportLimitExceeded {
                entity: "chapter artifact rows",
            });
        }
        if !identities.insert((row.1.clone(), row.2.clone(), row.3.clone(), row.4.clone())) {
            return Err(invalid(decoded.len(), "artifact identity is duplicated"));
        }
        let legacy_selected = selection(source_kind, row.11, decoded.len())?;
        let key = (row.1.clone(), row.2.clone());
        let importer_selected = legacy_selected
            .unwrap_or_else(|| row.9 == "available" && selected_subjects.insert(key.clone()));
        if importer_selected && legacy_selected == Some(true) && !selected_subjects.insert(key) {
            return Err(invalid(decoded.len(), "multiple artifacts are selected"));
        }
        let location = resolve_location(
            row.6.map(PathBuf::from),
            source_identity_path,
            rollback_format,
        )?;
        let mut item = LegacyChapterArtifactRow {
            row_id: u64::try_from(row.0)
                .map_err(|_| invalid(decoded.len(), "artifact row identity is invalid"))?,
            kind: row.1,
            subject: row.2,
            input_version: row.3,
            output_version: row.4,
            content_hash: row.5,
            location,
            origin: row.7,
            schema_version: row.8,
            integrity: row.9,
            verified_at_seconds: row.10,
            legacy_selected,
            importer_selected,
            row_digest: ContentDigest::default(),
        };
        item.row_digest = artifact_row_digest(&item);
        decoded.push(item);
    }
    decoded.sort_by_key(|row| (row.kind.clone(), row.subject.clone(), row.row_id));
    Ok(decoded)
}

fn selection(
    source_kind: LegacyChapterSourceKind,
    raw: Option<i64>,
    index: usize,
) -> Result<Option<bool>, StorageError> {
    match (source_kind, raw) {
        (LegacyChapterSourceKind::ArtifactSqliteV0, None) => Ok(None),
        (LegacyChapterSourceKind::ArtifactSqliteV1, Some(0)) => Ok(Some(false)),
        (LegacyChapterSourceKind::ArtifactSqliteV1, Some(1)) => Ok(Some(true)),
        _ => Err(invalid(index, "artifact selection flag is invalid")),
    }
}

fn resolve_location(
    location: Option<PathBuf>,
    source_identity_path: &Path,
    rollback_format: Option<i64>,
) -> Result<Option<PathBuf>, StorageError> {
    let Some(location) = location else {
        return Ok(None);
    };
    if location.is_absolute() {
        return Ok(Some(location));
    }
    if rollback_format != Some(1) {
        return Ok(Some(location));
    }
    let parent = source_identity_path
        .parent()
        .ok_or(StorageError::UnsupportedLegacySource)?;
    Ok(Some(parent.join(location)))
}

fn artifact_row_digest(row: &LegacyChapterArtifactRow) -> ContentDigest {
    let mut hash = TranscriptImportHash::new(b"pod0.legacy-chapter-row.v1");
    hash.u64(row.row_id);
    hash.text(&row.kind);
    hash.text(&row.subject);
    hash.text(&row.input_version);
    hash.text(&row.output_version);
    hash.text(&row.content_hash);
    hash.optional_text(row.location.as_ref().and_then(|path| path.to_str()));
    hash.optional_text(row.origin.as_deref());
    hash.i64(row.schema_version);
    hash.text(&row.integrity);
    hash.bytes(&row.verified_at_seconds.to_bits().to_be_bytes());
    hash.i64(row.legacy_selected.map_or(-1, i64::from));
    hash.u32(u32::from(row.importer_selected));
    hash.finish()
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, StorageError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
            [table],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("inspect chapter artifact table", error))
}

fn invalid(index: usize, detail: &'static str) -> StorageError {
    StorageError::InvalidLegacyRecord {
        entity: "chapter artifact",
        index: u32::try_from(index).unwrap_or(u32::MAX),
        detail,
    }
}
