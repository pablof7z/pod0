#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use pod0_domain::HostRequestId;
use rusqlite::params;

use crate::download_store_artifact_file::artifact_path;
#[cfg(test)]
use crate::download_store_artifact_file::{
    artifact_key, copy_and_hash_staged, install_staged, sync_parent,
};
use crate::download_store_request::u64_to_i64;
#[cfg(test)]
use crate::{DownloadArtifactBoundary, DownloadArtifactObserver, DownloadObservationOutcome};
use crate::{DownloadWorkflowRecord, LibraryStore, StorageError};
#[cfg(test)]
struct NoopObserver;

#[cfg(test)]
impl DownloadArtifactObserver for NoopObserver {
    fn reached(&self, _: DownloadArtifactBoundary) -> Result<(), StorageError> {
        Ok(())
    }
}

impl LibraryStore {
    #[cfg(test)]
    pub fn complete_download_from_staged_file(
        &self,
        request_id: HostRequestId,
        sequence_number: u64,
        staged_file_path: &str,
        claimed_byte_count: u64,
        observed_at_ms: i64,
    ) -> Result<DownloadObservationOutcome, StorageError> {
        self.complete_download_with_observer(
            request_id,
            sequence_number,
            staged_file_path,
            claimed_byte_count,
            observed_at_ms,
            &NoopObserver,
        )
    }

    #[cfg(test)]
    pub fn complete_download_with_observer(
        &self,
        request_id: HostRequestId,
        sequence_number: u64,
        staged_file_path: &str,
        claimed_byte_count: u64,
        observed_at_ms: i64,
        observer: &dyn DownloadArtifactObserver,
    ) -> Result<DownloadObservationOutcome, StorageError> {
        let Some((request, state)) = self.download_host_request(request_id)? else {
            return Ok(DownloadObservationOutcome::Stale);
        };
        let record = self
            .download_workflow(request.episode_id)?
            .ok_or(StorageError::DownloadWorkflowNotFound)?;
        if state != "pending"
            || request
                .last_sequence_number
                .is_some_and(|n| n >= sequence_number)
        {
            return Ok(DownloadObservationOutcome::Duplicate(record));
        }
        let attempt_id = request
            .attempt_id
            .ok_or(StorageError::StaleDownloadAttempt)?;
        if record.request_id != Some(request_id) || record.attempt_id != Some(attempt_id) {
            return Ok(DownloadObservationOutcome::Stale);
        }
        let source = Path::new(staged_file_path);
        let staged = match copy_and_hash_staged(self.path(), source, attempt_id, claimed_byte_count)
        {
            Ok(value) => value,
            Err(StorageError::InvalidDownloadArtifact) => {
                let failed = self.repair_invalid_artifact(&record, observed_at_ms)?;
                return Ok(DownloadObservationOutcome::Updated(failed));
            }
            Err(error) => return Err(error),
        };
        self.record_staged_artifact(
            &record,
            request_id,
            sequence_number,
            &staged.pending_path,
            staged.byte_count,
            staged.digest,
            observed_at_ms,
        )?;
        observer.reached(DownloadArtifactBoundary::AfterStagedRecord)?;
        let artifact_key = artifact_key(record.intent_id, record.attempt, staged.digest);
        let final_path = self.download_artifact_path(&artifact_key)?;
        install_staged(
            &staged.pending_path,
            &final_path,
            staged.byte_count,
            staged.digest,
        )?;
        sync_parent(&final_path)?;
        observer.reached(DownloadArtifactBoundary::AfterArtifactRename)?;
        let adopted = self.adopt_artifact(
            &record,
            request_id,
            sequence_number,
            &artifact_key,
            staged.byte_count,
            staged.digest,
            observed_at_ms,
        )?;
        Ok(DownloadObservationOutcome::Updated(adopted))
    }

    pub fn download_artifact_path(&self, artifact_key: &str) -> Result<PathBuf, StorageError> {
        artifact_path(self.path(), artifact_key)
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    fn record_staged_artifact(
        &self,
        record: &DownloadWorkflowRecord,
        request_id: HostRequestId,
        sequence_number: u64,
        path: &Path,
        byte_count: u64,
        digest: [u8; 32],
        now_ms: i64,
    ) -> Result<(), StorageError> {
        self.write(|transaction| {
            let path = path.to_str().ok_or(StorageError::InvalidDownloadArtifact)?;
            let changed = transaction.execute(
                "UPDATE pod0_download_attempts SET state='staged',staged_path=?1,\
                 staged_byte_count=?2,staged_digest=?3,updated_at_ms=?4 WHERE attempt_id=?5 \
                 AND request_id=?6 AND state IN('requested','host_accepted','transferring')",
                params![path,u64_to_i64(byte_count)?,digest.as_slice(),now_ms,
                    record.attempt_id.map(|id| id.into_bytes().to_vec()),request_id.into_bytes().as_slice()],
            ).map_err(|error| StorageError::sqlite("record staged download artifact", error))?;
            if changed != 1 { return Err(StorageError::StaleDownloadAttempt); }
            transaction.execute(
                "UPDATE pod0_download_host_requests SET last_sequence_number=?1,updated_at_ms=?2 \
                 WHERE request_id=?3 AND state='pending'",
                params![u64_to_i64(sequence_number)?,now_ms,request_id.into_bytes().as_slice()],
            ).map_err(|error| StorageError::sqlite("fence staged download observation", error))?;
            transaction.execute(
                "UPDATE pod0_download_workflows SET stage='staged',workflow_revision=workflow_revision+1,\
                 updated_at_ms=?1 WHERE episode_id=?2 AND request_id=?3 AND attempt_id=?4",
                params![now_ms,record.episode_id.into_bytes().as_slice(),request_id.into_bytes().as_slice(),
                    record.attempt_id.map(|id| id.into_bytes().to_vec())],
            ).map_err(|error| StorageError::sqlite("stage download workflow", error))?;
            if transaction.changes() != 1 { return Err(StorageError::StaleDownloadAttempt); }
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn adopt_artifact(
        &self,
        record: &DownloadWorkflowRecord,
        request_id: HostRequestId,
        sequence_number: u64,
        artifact_key: &str,
        byte_count: u64,
        digest: [u8; 32],
        now_ms: i64,
    ) -> Result<DownloadWorkflowRecord, StorageError> {
        let current = self
            .download_workflow(record.episode_id)?
            .ok_or(StorageError::DownloadWorkflowNotFound)?;
        if matches!(
            self.download_workflow_authority()?,
            crate::DownloadWorkflowAuthorityState::Staged { .. }
        ) {
            self.write(|transaction| {
                crate::transition_commit::download_artifact_recovery::apply_adoption(
                    transaction,
                    &current,
                    request_id,
                    sequence_number,
                    artifact_key,
                    byte_count,
                    digest,
                    now_ms,
                )
            })?;
            return self
                .download_workflow(current.episode_id)?
                .ok_or(StorageError::DownloadWorkflowNotFound);
        }
        crate::transition_commit::commit_download_artifact_recovery(
            self.path(),
            &current,
            crate::transition_commit::DownloadArtifactRecovery::Adopt {
                request_id,
                sequence_number,
                artifact_key: artifact_key.to_owned(),
                byte_count,
                digest,
            },
            now_ms,
        )
    }
}

pub(crate) fn complete_request(
    transaction: &rusqlite::Transaction<'_>,
    request_id: HostRequestId,
    sequence: u64,
    now_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_download_host_requests SET state='completed',last_sequence_number=\
         MAX(COALESCE(last_sequence_number,0),?1),updated_at_ms=?2 WHERE request_id=?3",
            params![
                u64_to_i64(sequence)?,
                now_ms,
                request_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("complete download host request", error))?;
    Ok(())
}
