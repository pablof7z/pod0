use std::path::{Path, PathBuf};

use pod0_domain::HostRequestId;
use rusqlite::params;

use crate::download_store_artifact_file::{
    artifact_key, artifact_path, copy_and_hash_staged, install_staged, sync_parent,
};
use crate::download_store_read::workflow;
use crate::download_store_request::u64_to_i64;
use crate::{
    DownloadArtifactBoundary, DownloadArtifactObserver, DownloadObservationOutcome,
    DownloadWorkflowRecord, LibraryStore, StorageError,
};

const ARTIFACT_SCHEMA_VERSION: i64 = 1;

struct NoopObserver;

impl DownloadArtifactObserver for NoopObserver {
    fn reached(&self, _: DownloadArtifactBoundary) -> Result<(), StorageError> {
        Ok(())
    }
}

impl LibraryStore {
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
        self.write(|transaction| {
            transaction.execute(
                "UPDATE pod0_download_attempts SET state='succeeded',staged_path=NULL,\
                 staged_byte_count=NULL,staged_digest=NULL,updated_at_ms=?1 \
                 WHERE attempt_id=?2 AND request_id=?3 AND state='staged'",
                params![now_ms,record.attempt_id.map(|id| id.into_bytes().to_vec()),request_id.into_bytes().as_slice()],
            ).map_err(|error| StorageError::sqlite("adopt download attempt", error))?;
            if transaction.changes()!=1 { return Err(StorageError::StaleDownloadAttempt); }
            complete_request(transaction,request_id,sequence_number,now_ms)?;
            transaction.execute(
                "UPDATE pod0_episodes SET download_code=2,download_wire_code=NULL,\
                 download_ref_version=?1,download_ref_key=?2,download_byte_count=?3 WHERE episode_id=?4",
                params![ARTIFACT_SCHEMA_VERSION,artifact_key,u64_to_i64(byte_count)?,record.episode_id.into_bytes().as_slice()],
            ).map_err(|error| StorageError::sqlite("adopt episode download artifact", error))?;
            if transaction.changes()!=1 { return Err(StorageError::EntityNotFound); }
            transaction.execute(
                "UPDATE pod0_download_workflows SET stage='succeeded',\
                 workflow_revision=workflow_revision+1,request_id=NULL,deadline_at_ms=NULL,\
                 not_before_ms=NULL,artifact_key=?1,artifact_byte_count=?2,artifact_digest=?3,\
                 failure_code=NULL,failure_detail=NULL,failure_retryable=0,updated_at_ms=?4 \
                 WHERE episode_id=?5 AND request_id=?6 AND attempt_id=?7 AND stage='staged'",
                params![artifact_key,u64_to_i64(byte_count)?,digest.as_slice(),now_ms,
                    record.episode_id.into_bytes().as_slice(),request_id.into_bytes().as_slice(),
                    record.attempt_id.map(|id| id.into_bytes().to_vec())],
            ).map_err(|error| StorageError::sqlite("complete download workflow", error))?;
            if transaction.changes()!=1 { return Err(StorageError::StaleDownloadAttempt); }
            workflow(transaction,record.episode_id)?.ok_or(StorageError::DownloadWorkflowNotFound)
        })
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
