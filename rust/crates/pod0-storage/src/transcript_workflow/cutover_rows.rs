use pod0_domain::ContentDigest;
use rusqlite::{Connection, OptionalExtension};

use super::cutover::{
    LegacyTranscriptWorkflowBackupRow, LegacyTranscriptWorkflowRowClassification,
};
use super::support::i64_value;
use crate::StorageError;

pub(super) type ImportManifest = (u64, ContentDigest, ContentDigest, u64, u32);
type RawImportManifest = (i64, Vec<u8>, Vec<u8>, i64, i64);

pub(super) fn import_manifest(
    connection: &Connection,
) -> Result<Option<ImportManifest>, StorageError> {
    let row: Option<RawImportManifest> = connection
        .query_row(
            "SELECT source_generation,source_fingerprint,backup_digest,backup_byte_count,row_count
             FROM pod0_transcript_workflow_imports WHERE singleton=1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read transcript workflow import manifest", error))?;
    row.map(|(generation, source, backup, bytes, count)| {
        Ok((
            u64::try_from(generation).map_err(|_| StorageError::TranscriptWorkflowConflict)?,
            super::support::digest(source)?,
            super::support::digest(backup)?,
            u64::try_from(bytes).map_err(|_| StorageError::TranscriptWorkflowConflict)?,
            u32::try_from(count).map_err(|_| StorageError::TranscriptWorkflowConflict)?,
        ))
    })
    .transpose()
}

pub(super) fn read_rows(
    connection: &Connection,
    source_generation: u64,
) -> Result<Vec<LegacyTranscriptWorkflowBackupRow>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT episode_id,row_bytes,row_fingerprint,classification
             FROM pod0_transcript_workflow_import_rows WHERE source_generation=?1
             ORDER BY ordinal",
        )
        .map_err(|error| {
            StorageError::sqlite("prepare transcript workflow rollback rows", error)
        })?;
    let rows = statement
        .query_map([i64_value(source_generation)?], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read transcript workflow rollback rows", error))?;
    rows.map(|row| {
        let (episode, bytes, fingerprint, classification) = row.map_err(|error| {
            StorageError::sqlite("decode transcript workflow rollback row", error)
        })?;
        Ok(LegacyTranscriptWorkflowBackupRow {
            episode_id: pod0_domain::EpisodeId::from_bytes(super::support::bytes16(episode)?),
            row_bytes: bytes,
            row_fingerprint: super::support::digest(fingerprint)?,
            classification: LegacyTranscriptWorkflowRowClassification::parse(&classification)
                .ok_or(StorageError::TranscriptWorkflowConflict)?,
        })
    })
    .collect()
}
