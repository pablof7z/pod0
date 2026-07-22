use pod0_domain::ContentDigest;
use rusqlite::{Connection, params};
use sha2::{Digest as _, Sha256};

use super::authority::read_authority;
use super::cutover::*;
use super::cutover_adoption::adopt_candidate;
use super::cutover_rows::{import_manifest, read_rows};
use super::cutover_validation::{insert_backup_rows, insert_manifest, validate_input};
use super::support::i64_value;
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn stage_legacy_transcript_workflow_cutover(
        &self,
        input: LegacyTranscriptWorkflowCutoverInput,
    ) -> Result<LegacyTranscriptWorkflowCutoverReport, StorageError> {
        validate_input(&input)?;
        self.write(|transaction| {
            match read_authority(transaction)? {
                TranscriptWorkflowAuthorityState::NotStarted => {}
                TranscriptWorkflowAuthorityState::Staged { source_generation }
                | TranscriptWorkflowAuthorityState::Verified { source_generation }
                | TranscriptWorkflowAuthorityState::Authoritative { source_generation }
                    if source_generation == input.source_generation =>
                {
                    verify_stored_import(
                        transaction,
                        input.source_generation,
                        input.source_fingerprint,
                    )?;
                    return cutover_report(transaction, input.source_generation);
                }
                _ => return Err(StorageError::TranscriptWorkflowConflict),
            }
            let existing: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pod0_transcript_workflows LIMIT 1)",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| {
                    StorageError::sqlite("inspect transcript workflows before cutover", error)
                })?;
            if existing {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
            insert_manifest(transaction, &input)?;
            insert_backup_rows(transaction, &input)?;
            for candidate in &input.candidates {
                adopt_candidate(transaction, &input, candidate)?;
            }
            transaction
                .execute(
                    "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,core_revision,committed_at_ms)
                     VALUES('transcript_workflows','staged',?1,?2,?3)",
                    params![
                        i64_value(input.source_generation)?,
                        i64_value(input.issued_revision.value)?,
                        input.now_ms
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("stage transcript workflow cutover", error)
                })?;
            cutover_report(transaction, input.source_generation)
        })
    }
}

pub fn transcript_workflow_source_fingerprint(
    rows: &[LegacyTranscriptWorkflowBackupRow],
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0-legacy-transcript-workflow-source-v1");
    for (ordinal, row) in rows.iter().enumerate() {
        hash.update((ordinal as u64).to_be_bytes());
        hash.update(row.episode_id.into_bytes());
        hash.update(row.row_fingerprint.into_bytes());
        hash.update(row.classification.wire().as_bytes());
    }
    ContentDigest::from_bytes(hash.finalize().into())
}

pub(super) fn verify_stored_import(
    connection: &Connection,
    source_generation: u64,
    expected_fingerprint: ContentDigest,
) -> Result<(), StorageError> {
    let Some((stored_generation, stored_fingerprint, _, _, row_count)) =
        import_manifest(connection)?
    else {
        return Err(StorageError::TranscriptWorkflowConflict);
    };
    let rows = read_rows(connection, source_generation)?;
    let workflow_mismatch: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pod0_transcript_workflows
             WHERE source_generation IS NULL OR source_generation<>?1)",
            [i64_value(source_generation)?],
            |row| row.get(0),
        )
        .map_err(|error| {
            StorageError::sqlite("verify staged transcript workflow ownership", error)
        })?;
    if stored_generation != source_generation
        || stored_fingerprint != expected_fingerprint
        || transcript_workflow_source_fingerprint(&rows) != expected_fingerprint
        || rows.len() != row_count as usize
        || workflow_mismatch
    {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    Ok(())
}

pub(super) fn cutover_report(
    connection: &Connection,
    source_generation: u64,
) -> Result<LegacyTranscriptWorkflowCutoverReport, StorageError> {
    let (_, source_fingerprint, _, _, row_count) =
        import_manifest(connection)?.ok_or(StorageError::TranscriptWorkflowConflict)?;
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_transcript_workflows WHERE source_generation=?1",
            [i64_value(source_generation)?],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("count adopted transcript workflows", error))?;
    Ok(LegacyTranscriptWorkflowCutoverReport {
        state: read_authority(connection)?,
        source_fingerprint,
        row_count,
        adopted_workflow_count: u32::try_from(count)
            .map_err(|_| StorageError::TranscriptWorkflowConflict)?,
    })
}

pub(super) fn read_rollback_export(
    connection: &Connection,
) -> Result<TranscriptWorkflowRollbackExport, StorageError> {
    let (source_generation, source_fingerprint, backup_digest, backup_byte_count, _) =
        import_manifest(connection)?.ok_or(StorageError::TranscriptWorkflowConflict)?;
    Ok(TranscriptWorkflowRollbackExport {
        source_generation,
        source_fingerprint,
        backup_digest,
        backup_byte_count,
        rows: read_rows(connection, source_generation)?,
    })
}
