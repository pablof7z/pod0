use crate::chapter_rollback_export::CHAPTER_ROLLBACK_FORMAT_VERSION;
use crate::chapter_rollback_format::{ChapterRollbackEntry, ChapterRollbackManifest};
use crate::transcript_import_digest::{hex_digest, parse_hex_digest};
use pod0_domain::ContentDigest;

use crate::{CURRENT_SCHEMA_VERSION, ChapterImportReport, StorageError};

pub(crate) fn build_manifest(
    connection: &rusqlite::Connection,
    report: &ChapterImportReport,
) -> Result<ChapterRollbackManifest, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT entry_id,evidence_kind,source_subject,source_row_id,raw_digest,raw_byte_count,\
             importer_selected,validation_state,diagnostic_code,artifact_id \
             FROM pod0_chapter_import_entries WHERE import_id=?1 ORDER BY entry_id",
        )
        .map_err(|error| StorageError::sqlite("prepare chapter rollback entries", error))?;
    let rows = statement
        .query_map([report.import_id.into_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, bool>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<Vec<u8>>>(9)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read chapter rollback entries", error))?;
    let entries = rows
        .map(|row| {
            let row =
                row.map_err(|error| StorageError::sqlite("decode chapter rollback entry", error))?;
            let raw_digest = crate::chapter_store_codec::digest(&row.4)?;
            Ok(ChapterRollbackEntry {
                evidence_id: hex(&row.0),
                evidence_kind: row.1,
                source_subject: row.2,
                source_row_id: row
                    .3
                    .map(|value| u64::try_from(value).map_err(|_| StorageError::BackupConflict))
                    .transpose()?,
                raw_digest: hex_digest(raw_digest),
                raw_byte_count: u64::try_from(row.5).map_err(|_| StorageError::BackupConflict)?,
                relative_path: format!("evidence/{}.json", hex_digest(raw_digest)),
                importer_selected: row.6,
                validation_state: row.7,
                diagnostic_code: row.8,
                artifact_id: row.9.map(|value| hex(&value)),
            })
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    Ok(ChapterRollbackManifest {
        format_version: CHAPTER_ROLLBACK_FORMAT_VERSION,
        core_schema_version: CURRENT_SCHEMA_VERSION,
        source_kind: report.plan.source_kind.code().to_owned(),
        source_generation: report.plan.source_generation,
        source_database_digest: hex_digest(report.plan.source_database_digest),
        source_selection_digest: hex_digest(report.plan.source_selection_digest),
        evidence_count: report.plan.evidence_count,
        artifact_count: report.plan.canonical_artifact_count,
        selected_count: report.plan.selected_count,
        blocked_count: report.plan.blocked_count,
        original_database_path: "original-source.sqlite".to_owned(),
        database_path: "source.sqlite".to_owned(),
        entries,
    })
}

pub(crate) fn parse_digest(value: &str) -> Result<ContentDigest, StorageError> {
    parse_hex_digest(value).map_err(|_| StorageError::BackupConflict)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
