use pod0_domain::ContentDigest;
use rusqlite::params;

use super::authority::read_authority;
pub use super::cutover_model::*;
use super::support::i64_value;
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn transcript_workflow_cutover_report(
        &self,
    ) -> Result<Option<LegacyTranscriptWorkflowCutoverReport>, StorageError> {
        self.read(|connection| {
            let source_generation = match read_authority(connection)? {
                TranscriptWorkflowAuthorityState::NotStarted => return Ok(None),
                TranscriptWorkflowAuthorityState::Staged { source_generation }
                | TranscriptWorkflowAuthorityState::Verified { source_generation }
                | TranscriptWorkflowAuthorityState::Authoritative { source_generation } => {
                    source_generation
                }
            };
            super::cutover_stage::cutover_report(connection, source_generation).map(Some)
        })
    }

    pub fn transcript_workflow_authority(
        &self,
    ) -> Result<TranscriptWorkflowAuthorityState, StorageError> {
        self.read(read_authority)
    }

    pub fn verify_legacy_transcript_workflow_cutover(
        &self,
        source_generation: u64,
        source_fingerprint: ContentDigest,
        verified_at_ms: i64,
    ) -> Result<LegacyTranscriptWorkflowCutoverReport, StorageError> {
        if source_generation == 0 || verified_at_ms < 0 {
            return Err(StorageError::TranscriptWorkflowConflict);
        }
        self.write(|transaction| {
            super::cutover_stage::verify_stored_import(
                transaction,
                source_generation,
                source_fingerprint,
            )?;
            transaction
                .execute(
                    "UPDATE pod0_transcript_workflow_imports SET state='verified',verified_at_ms=?1
                 WHERE singleton=1 AND source_generation=?2 AND source_fingerprint=?3
                 AND state IN('staged','verified')",
                    params![
                        verified_at_ms,
                        i64_value(source_generation)?,
                        source_fingerprint.into_bytes().as_slice()
                    ],
                )
                .map_err(|error| {
                    StorageError::sqlite("verify transcript workflow cutover", error)
                })?;
            super::cutover_stage::cutover_report(transaction, source_generation)
        })
    }

    pub fn commit_legacy_transcript_workflow_cutover(
        &self,
        source_generation: u64,
        verified_source_fingerprint: ContentDigest,
        committed_at_ms: i64,
    ) -> Result<TranscriptWorkflowAuthorityState, StorageError> {
        self.commit_legacy_transcript_workflow_cutover_with_observer(
            source_generation,
            verified_source_fingerprint,
            committed_at_ms,
            || Ok(()),
        )
    }

    pub(crate) fn commit_legacy_transcript_workflow_cutover_with_observer<F>(
        &self,
        source_generation: u64,
        verified_source_fingerprint: ContentDigest,
        committed_at_ms: i64,
        before_commit: F,
    ) -> Result<TranscriptWorkflowAuthorityState, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        if source_generation == 0 || committed_at_ms < 0 {
            return Err(StorageError::TranscriptWorkflowConflict);
        }
        self.write(|transaction| {
            match read_authority(transaction)? {
                TranscriptWorkflowAuthorityState::Authoritative { source_generation: current }
                    if current == source_generation => return Ok(TranscriptWorkflowAuthorityState::Authoritative { source_generation }),
                TranscriptWorkflowAuthorityState::Verified { source_generation: current }
                    if current == source_generation => {}
                _ => return Err(StorageError::TranscriptWorkflowConflict),
            }
            super::cutover_stage::verify_stored_import(transaction,source_generation,verified_source_fingerprint)?;
            let changed = transaction.execute(
                "UPDATE pod0_transcript_workflow_imports SET state='authoritative',committed_at_ms=?1
                 WHERE singleton=1 AND source_generation=?2 AND source_fingerprint=?3 AND state='verified'",
                params![committed_at_ms,i64_value(source_generation)?,verified_source_fingerprint.into_bytes().as_slice()],
            ).map_err(|error| StorageError::sqlite("commit transcript workflow import", error))?;
            if changed != 1 { return Err(StorageError::TranscriptWorkflowConflict); }
            let changed = transaction.execute(
                "UPDATE pod0_domain_cutovers SET state='authoritative',committed_at_ms=?1
                 WHERE domain='transcript_workflows' AND source_generation=?2 AND state='staged'",
                params![committed_at_ms,i64_value(source_generation)?],
            ).map_err(|error| StorageError::sqlite("commit transcript workflow authority", error))?;
            if changed != 1 { return Err(StorageError::TranscriptWorkflowConflict); }
            before_commit()?;
            Ok(TranscriptWorkflowAuthorityState::Authoritative { source_generation })
        })
    }

    pub fn discard_legacy_transcript_workflow_cutover(
        &self,
        source_generation: u64,
    ) -> Result<bool, StorageError> {
        self.write(|transaction| match read_authority(transaction)? {
            TranscriptWorkflowAuthorityState::Staged { source_generation: current }
            | TranscriptWorkflowAuthorityState::Verified { source_generation: current }
                if current == source_generation => {
                    transaction.execute("DELETE FROM pod0_transcript_workflows WHERE source_generation=?1",
                        [i64_value(source_generation)?]).map_err(|error| StorageError::sqlite("discard staged transcript workflows", error))?;
                    transaction.execute("DELETE FROM pod0_domain_cutovers WHERE domain='transcript_workflows' AND source_generation=?1 AND state='staged'",
                        [i64_value(source_generation)?]).map_err(|error| StorageError::sqlite("discard transcript workflow cutover", error))?;
                    transaction.execute("DELETE FROM pod0_transcript_workflow_imports WHERE singleton=1 AND source_generation=?1",
                        [i64_value(source_generation)?]).map_err(|error| StorageError::sqlite("discard transcript workflow import", error))?;
                    Ok(true)
                }
            TranscriptWorkflowAuthorityState::NotStarted => Ok(false),
            _ => Err(StorageError::TranscriptWorkflowConflict),
        })
    }

    pub fn export_legacy_transcript_workflow_rollback(
        &self,
    ) -> Result<TranscriptWorkflowRollbackExport, StorageError> {
        self.read(|connection| {
            if read_authority(connection)?.is_authoritative() {
                return Err(StorageError::TranscriptWorkflowConflict);
            }
            super::cutover_stage::read_rollback_export(connection)
        })
    }
}
