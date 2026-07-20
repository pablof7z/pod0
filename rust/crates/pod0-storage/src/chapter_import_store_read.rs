use std::path::Path;

use pod0_domain::{CommandId, StateRevision};
use rusqlite::{Connection, OptionalExtension};

use crate::chapter_store_codec::{artifact_id, digest, stored_u32, stored_u64};
use crate::migration_db::{open_connection, user_version, validate_open_database};
use crate::{
    CURRENT_SCHEMA_VERSION, ChapterBackupEvidence, ChapterEvidenceValidation, ChapterImportPlan,
    ChapterImportReport, ChapterImportState, LegacyChapterSourceKind, StorageError,
    StoredChapterEvidence,
};

pub fn read_chapter_import(
    target_path: &Path,
    import_id: CommandId,
) -> Result<ChapterImportReport, StorageError> {
    let connection = open_current(target_path)?;
    read_import_report(&connection, import_id, true)?.ok_or(StorageError::ChapterImportNotFound)
}

pub fn read_active_chapter_import(
    target_path: &Path,
) -> Result<Option<ChapterImportReport>, StorageError> {
    let connection = open_current(target_path)?;
    let bytes: Option<Vec<u8>> = connection
        .query_row(
            "SELECT import_id FROM pod0_chapter_imports WHERE state <> 'discarded' \
             ORDER BY CASE state WHEN 'staged' THEN 0 WHEN 'verified' THEN 0 \
             WHEN 'corrupt' THEN 0 ELSE 1 END,staged_at_ms DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read active chapter import", error))?;
    bytes
        .map(|bytes| {
            let value: [u8; 16] = bytes.try_into().map_err(|_| corrupt("chapter import ID"))?;
            read_import_report(&connection, CommandId::from_bytes(value), true)?
                .ok_or(StorageError::ChapterImportNotFound)
        })
        .transpose()
}

#[allow(clippy::type_complexity)]
pub(crate) fn read_import_report(
    connection: &Connection,
    import_id: CommandId,
    reused_existing: bool,
) -> Result<Option<ChapterImportReport>, StorageError> {
    let row = connection
        .query_row(
            "SELECT source_kind,source_identity,source_generation,source_byte_count,\
             source_database_digest,source_selection_digest,evidence_count,artifact_count,\
             selected_count,blocked_count,target_revision,state,backup_database_digest,\
             backup_database_byte_count,backup_file_count,backup_file_byte_count,diagnostic_code \
             FROM pod0_chapter_imports WHERE import_id=?1",
            [import_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, Vec<u8>>(12)?,
                    row.get::<_, i64>(13)?,
                    row.get::<_, i64>(14)?,
                    row.get::<_, i64>(15)?,
                    row.get::<_, Option<String>>(16)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read chapter import", error))?;
    let Some(row) = row else { return Ok(None) };
    let source_kind = LegacyChapterSourceKind::from_code(&row.0)
        .ok_or_else(|| corrupt("chapter import source kind"))?;
    let report = ChapterImportReport {
        import_id,
        plan: ChapterImportPlan {
            source_kind,
            source_file_identity: digest(&row.1)?,
            source_generation: stored_u64(row.2, "chapter source generation")?,
            source_database_byte_count: stored_u64(row.3, "chapter source bytes")?,
            source_database_digest: digest(&row.4)?,
            source_selection_digest: digest(&row.5)?,
            evidence_count: stored_u32(row.6, "chapter evidence count")?,
            canonical_artifact_count: stored_u32(row.7, "chapter artifact count")?,
            selected_count: stored_u32(row.8, "selected chapter count")?,
            blocked_count: stored_u32(row.9, "blocked chapter count")?,
        },
        target_revision: StateRevision::new(stored_u64(row.10, "chapter target revision")?),
        backup: ChapterBackupEvidence {
            database_digest: digest(&row.12)?,
            database_byte_count: stored_u64(row.13, "chapter backup database bytes")?,
            file_count: stored_u32(row.14, "chapter backup file count")?,
            file_byte_count: stored_u64(row.15, "chapter backup file bytes")?,
            reused_database: reused_existing,
            reused_files: if reused_existing {
                stored_u32(row.14, "chapter backup file count")?
            } else {
                0
            },
        },
        state: ChapterImportState::from_code(&row.11)
            .ok_or_else(|| corrupt("chapter import state"))?,
        diagnostic_code: row.16,
        reused_existing,
    };
    verify_report_aggregates(connection, &report)?;
    Ok(Some(report))
}

pub(crate) fn read_import_fingerprint(
    connection: &Connection,
    import_id: CommandId,
) -> Result<Option<pod0_domain::ContentDigest>, StorageError> {
    connection
        .query_row(
            "SELECT command_fingerprint FROM pod0_chapter_imports WHERE import_id=?1",
            [import_id.into_bytes().as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read chapter import fingerprint", error))?
        .map(|bytes| digest(&bytes))
        .transpose()
}

pub(crate) fn read_import_entries(
    connection: &Connection,
    import_id: CommandId,
) -> Result<Vec<StoredChapterEvidence>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT entry_id,raw_digest,raw_byte_count,artifact_id,validation_state \
             FROM pod0_chapter_import_entries WHERE import_id=?1 ORDER BY entry_id",
        )
        .map_err(|error| StorageError::sqlite("prepare chapter import entries", error))?;
    let rows = statement
        .query_map([import_id.into_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<Vec<u8>>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read chapter import entries", error))?;
    rows.map(|row| {
        let row =
            row.map_err(|error| StorageError::sqlite("decode chapter import entry", error))?;
        Ok(StoredChapterEvidence {
            evidence_id: digest(&row.0)?,
            raw_digest: digest(&row.1)?,
            raw_byte_count: stored_u64(row.2, "chapter evidence bytes")?,
            artifact_id: row.3.as_deref().map(artifact_id).transpose()?,
            validation: validation(&row.4)?,
        })
    })
    .collect()
}

pub(crate) fn open_current(path: &Path) -> Result<Connection, StorageError> {
    let connection = open_connection(path, false)?;
    let version = user_version(&connection)?;
    validate_open_database(&connection, version)?;
    if version == CURRENT_SCHEMA_VERSION {
        Ok(connection)
    } else {
        Err(corrupt("chapter import schema is not current"))
    }
}

fn verify_report_aggregates(
    connection: &Connection,
    report: &ChapterImportReport,
) -> Result<(), StorageError> {
    let counts: (i64, i64, i64) = connection
        .query_row(
            "SELECT COUNT(*),COALESCE(SUM(CASE WHEN validation_state='blocked' THEN 1 ELSE 0 END),0),\
             COALESCE(SUM(CASE WHEN importer_selected=1 AND evidence_kind IN \
             ('episode_adjunct','workflow_chapters') THEN 1 ELSE 0 END),0) \
             FROM pod0_chapter_import_entries WHERE import_id=?1",
            [report.import_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| StorageError::sqlite("read chapter import aggregates", error))?;
    let artifact_count: i64 = connection
        .query_row(
            "SELECT COUNT(DISTINCT artifact_id) FROM pod0_chapter_import_entries \
             WHERE import_id=?1 AND artifact_id IS NOT NULL",
            [report.import_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read chapter artifact aggregates", error))?;
    let expected = &report.plan;
    if stored_u32(counts.0, "chapter entry count")? != expected.evidence_count
        || stored_u32(counts.1, "blocked chapter entry count")? != expected.blocked_count
        || stored_u32(counts.2, "selected chapter entry count")? != expected.selected_count
        || stored_u32(artifact_count, "chapter artifact aggregate count")?
            != expected.canonical_artifact_count
    {
        return Err(corrupt("chapter import aggregates differ"));
    }
    Ok(())
}

fn validation(value: &str) -> Result<ChapterEvidenceValidation, StorageError> {
    match value {
        "canonical" => Ok(ChapterEvidenceValidation::Canonical),
        "inert" => Ok(ChapterEvidenceValidation::Inert),
        "blocked" => Ok(ChapterEvidenceValidation::Blocked),
        _ => Err(corrupt("chapter evidence validation state")),
    }
}

fn corrupt(detail: &'static str) -> StorageError {
    StorageError::CorruptSchema { detail }
}
