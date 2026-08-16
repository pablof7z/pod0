use std::fs;
use std::path::Path;

use rusqlite::OptionalExtension;

use crate::download_store_artifact_file::{
    artifact_key, file_metadata_matches, install_staged, sync_parent, verified_file,
};
use crate::{
    DownloadRecoveryReport, DownloadWorkflowRecord, LibraryStore, StorageError, StoredDownloadStage,
};

impl LibraryStore {
    pub fn recover_download_artifacts(&self) -> Result<DownloadRecoveryReport, StorageError> {
        let records = self.download_workflow_page(None, 0, u16::MAX)?.items;
        let mut report = DownloadRecoveryReport::default();
        for record in records {
            match record.stage {
                StoredDownloadStage::Staged => {
                    if self.recover_staged_record(&record)? {
                        report.adopted_count = report.adopted_count.saturating_add(1);
                    } else {
                        self.repair_invalid_artifact(&record, record.updated_at_ms)?;
                        report.repaired_count = report.repaired_count.saturating_add(1);
                    }
                }
                StoredDownloadStage::Succeeded => {
                    if !self.succeeded_artifact_is_valid(&record)? {
                        if let Some(key) = record.artifact_key.as_deref()
                            && let Ok(path) = self.download_artifact_path(key)
                        {
                            let _ = fs::remove_file(path);
                        }
                        self.repair_invalid_artifact(&record, record.updated_at_ms)?;
                        report.repaired_count = report.repaired_count.saturating_add(1);
                    }
                }
                _ => {}
            }
        }
        Ok(report)
    }

    fn recover_staged_record(&self, record: &DownloadWorkflowRecord) -> Result<bool, StorageError> {
        let Some(attempt_id) = record.attempt_id else {
            return Ok(false);
        };
        let staged: Option<(String, i64, Vec<u8>)> = self.read(|connection| {
            connection.query_row(
            "SELECT staged_path,staged_byte_count,staged_digest FROM pod0_download_attempts \
             WHERE attempt_id=?1 AND state='staged'",
            [attempt_id.into_bytes().as_slice()],|row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        ).optional().map_err(|error| StorageError::sqlite("read staged download recovery",error))
        })?;
        let Some((pending, count, digest)) = staged else {
            return Ok(false);
        };
        let digest: [u8; 32] = digest
            .try_into()
            .map_err(|_| StorageError::InvalidDownloadArtifact)?;
        let count = u64::try_from(count).map_err(|_| StorageError::InvalidDownloadArtifact)?;
        let key = artifact_key(record.intent_id, record.attempt, digest);
        let final_path = self.download_artifact_path(&key)?;
        if !verified_file(&final_path, count, digest)? {
            let pending = Path::new(&pending);
            if !verified_file(pending, count, digest)? {
                let _ = fs::remove_file(pending);
                return Ok(false);
            }
            install_staged(pending, &final_path, count, digest)?;
            sync_parent(&final_path)?;
        }
        let request_id = record
            .request_id
            .ok_or(StorageError::StaleDownloadAttempt)?;
        let sequence = self
            .download_host_request(request_id)?
            .and_then(|(request, _)| request.last_sequence_number)
            .unwrap_or(0);
        self.adopt_artifact(
            record,
            request_id,
            sequence,
            &key,
            count,
            digest,
            record.updated_at_ms,
        )?;
        Ok(true)
    }

    fn succeeded_artifact_is_valid(
        &self,
        record: &DownloadWorkflowRecord,
    ) -> Result<bool, StorageError> {
        let (Some(key), Some(count), Some(digest)) = (
            &record.artifact_key,
            record.artifact_byte_count,
            record.artifact_digest,
        ) else {
            return Ok(false);
        };
        // A succeeded artifact was fully hashed before its atomic rename and
        // the digest is part of its typed key. Re-reading every downloaded
        // episode on each launch makes startup O(total media bytes), so
        // restart recovery only validates the immutable identity and file
        // metadata. Interrupted staged files above still receive a full hash.
        if key != &artifact_key(record.intent_id, record.attempt, digest) {
            return Ok(false);
        }
        file_metadata_matches(&self.download_artifact_path(key)?, count)
    }

    pub(crate) fn repair_invalid_artifact(
        &self,
        record: &DownloadWorkflowRecord,
        now_ms: i64,
    ) -> Result<DownloadWorkflowRecord, StorageError> {
        if matches!(
            self.download_workflow_authority()?,
            crate::DownloadWorkflowAuthorityState::Staged { .. }
        ) {
            self.write(|transaction| {
                crate::transition_commit::download_artifact_recovery::apply_repair(
                    transaction,
                    record,
                    now_ms,
                )
            })?;
            return self
                .download_workflow(record.episode_id)?
                .ok_or(StorageError::DownloadWorkflowNotFound);
        }
        crate::transition_commit::commit_download_artifact_recovery(
            self.path(),
            record,
            crate::transition_commit::DownloadArtifactRecovery::RepairInvalid,
            now_ms,
        )
    }
}
