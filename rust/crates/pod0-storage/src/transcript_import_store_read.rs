use std::path::Path;

use pod0_domain::{CommandId, StateRevision};
use rusqlite::{Connection, OptionalExtension, params};

use crate::migration_db::{open_connection, user_version, validate_open_database};
use crate::transcript_import_model::{
    LegacyTranscriptSourceKind, StoredTranscriptImportEntry, TranscriptBackupEvidence,
    TranscriptImportEntrySummary, TranscriptImportPlan, TranscriptImportReport,
    TranscriptImportState,
};
use crate::transcript_store_codec::{artifact_id, digest, episode_id, version_id};
use crate::transcript_store_model::TranscriptPage;
use crate::transcript_store_read_rows::{finish_page, page_limit};
use crate::{CURRENT_SCHEMA_VERSION, StorageError};

pub fn read_transcript_import(
    target_path: &Path,
    import_id: CommandId,
) -> Result<TranscriptImportReport, StorageError> {
    let connection = open_current(target_path)?;
    read_import_report(&connection, import_id, true)?.ok_or(StorageError::TranscriptImportNotFound)
}

pub fn read_transcript_import_entries(
    target_path: &Path,
    import_id: CommandId,
    offset: u32,
    max_items: u16,
) -> Result<TranscriptPage<TranscriptImportEntrySummary>, StorageError> {
    let connection = open_current(target_path)?;
    if read_import_report(&connection, import_id, true)?.is_none() {
        return Err(StorageError::TranscriptImportNotFound);
    }
    let (limit, fetch) = page_limit(max_items);
    let mut statement = connection
        .prepare(
            "SELECT e.episode_id,e.selected_row_digest,e.selected_file_digest,e.artifact_id,\
             e.transcript_version_id,d.content_digest FROM pod0_transcript_import_entries e \
             JOIN pod0_transcript_documents d ON d.transcript_version_id=e.transcript_version_id \
             WHERE e.import_id=?1 ORDER BY e.episode_id LIMIT ?2 OFFSET ?3",
        )
        .map_err(|error| {
            StorageError::sqlite("prepare bounded transcript import entries", error)
        })?;
    let rows = statement
        .query_map(
            params![import_id.into_bytes().as_slice(), fetch, i64::from(offset)],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                ))
            },
        )
        .map_err(|error| StorageError::sqlite("read bounded transcript import entries", error))?;
    let items = rows
        .map(|row| {
            let row = row.map_err(|error| {
                StorageError::sqlite("decode transcript import entry summary", error)
            })?;
            Ok(TranscriptImportEntrySummary {
                episode_id: episode_id(&row.0)?,
                selected_row_digest: digest(&row.1)?,
                selected_file_digest: digest(&row.2)?,
                artifact_id: artifact_id(&row.3)?,
                transcript_version_id: version_id(&row.4)?,
                transcript_content_digest: digest(&row.5)?,
            })
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    Ok(finish_page(items, limit))
}

#[allow(clippy::type_complexity)]
pub(crate) fn read_import_report(
    connection: &Connection,
    import_id: CommandId,
    reused_existing: bool,
) -> Result<Option<TranscriptImportReport>, StorageError> {
    let row = connection
        .query_row(
            "SELECT source_kind,source_generation,source_database_digest,\
             source_selection_digest,backup_database_digest,backup_database_byte_count,\
             selected_count,target_revision,state,diagnostic_code FROM pod0_transcript_imports \
             WHERE import_id=?1",
            [import_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<String>>(9)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read transcript import", error))?;
    let Some(row) = row else { return Ok(None) };
    let source_kind = LegacyTranscriptSourceKind::from_code(&row.0)
        .ok_or_else(|| corrupt("transcript import source kind is malformed"))?;
    let selected_count =
        u32::try_from(row.6).map_err(|_| corrupt("transcript import count is malformed"))?;
    let aggregates: (i64, i64) = connection
        .query_row(
            "SELECT COUNT(*),COALESCE(SUM(backup_file_byte_count),0) \
             FROM pod0_transcript_import_entries WHERE import_id=?1",
            [import_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| StorageError::sqlite("read transcript import aggregates", error))?;
    if u32::try_from(aggregates.0).ok() != Some(selected_count) {
        return Err(corrupt("transcript import entry count differs"));
    }
    let artifact_byte_count = stored_u64(aggregates.1, "transcript import backup bytes overflow")?;
    Ok(Some(TranscriptImportReport {
        import_id,
        plan: TranscriptImportPlan {
            source_kind,
            source_generation: stored_u64(row.1, "transcript source generation")?,
            source_database_digest: digest(&row.2)?,
            source_selection_digest: digest(&row.3)?,
            selected_count,
        },
        target_revision: StateRevision::new(stored_u64(row.7, "transcript target revision")?),
        backup: TranscriptBackupEvidence {
            database_digest: digest(&row.4)?,
            database_byte_count: stored_u64(row.5, "transcript database backup bytes")?,
            artifact_count: selected_count,
            artifact_byte_count,
            reused_database: reused_existing,
            reused_artifacts: if reused_existing { selected_count } else { 0 },
        },
        state: TranscriptImportState::from_code(&row.8)
            .ok_or_else(|| corrupt("transcript import state is malformed"))?,
        diagnostic_code: row.9,
        reused_existing,
    }))
}

pub(crate) fn read_import_entries(
    connection: &Connection,
    import_id: CommandId,
) -> Result<Vec<StoredTranscriptImportEntry>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT episode_id,legacy_row_id,selected_row_digest,selected_file_digest,\
             backup_file_digest,backup_file_byte_count,artifact_id,transcript_version_id \
             FROM pod0_transcript_import_entries WHERE import_id=?1 ORDER BY episode_id",
        )
        .map_err(|error| StorageError::sqlite("prepare transcript import entries", error))?;
    let rows = statement
        .query_map([import_id.into_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Vec<u8>>(6)?,
                row.get::<_, Vec<u8>>(7)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read transcript import entries", error))?;
    rows.map(|row| {
        let row =
            row.map_err(|error| StorageError::sqlite("decode transcript import entry", error))?;
        Ok(StoredTranscriptImportEntry {
            episode_id: episode_id(&row.0)?,
            legacy_row_id: stored_u64(row.1, "transcript legacy row identity")?,
            selected_row_digest: digest(&row.2)?,
            selected_file_digest: digest(&row.3)?,
            backup_file_digest: digest(&row.4)?,
            backup_file_byte_count: stored_u64(row.5, "transcript artifact backup bytes")?,
            artifact_id: artifact_id(&row.6)?,
            transcript_version_id: version_id(&row.7)?,
        })
    })
    .collect()
}

pub(crate) fn open_current(path: &Path) -> Result<Connection, StorageError> {
    let connection = open_connection(path, false)?;
    let version = user_version(&connection)?;
    validate_open_database(&connection, version)?;
    if version != CURRENT_SCHEMA_VERSION {
        return Err(corrupt("transcript import schema is not current"));
    }
    Ok(connection)
}

fn stored_u64(value: i64, detail: &'static str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| corrupt(detail))
}

fn corrupt(detail: &'static str) -> StorageError {
    StorageError::CorruptSchema { detail }
}
