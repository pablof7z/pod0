use pod0_domain::{DownloadAttemptId, HostRequestId, StateRevision};
use rusqlite::params;

use super::download_finalization::Finalization;
use crate::{DownloadWorkflowRecord, StorageError};

pub(super) fn apply(
    transaction: &rusqlite::Transaction<'_>,
    current: &DownloadWorkflowRecord,
    request_id: HostRequestId,
    finalization: Finalization,
    now_ms: i64,
) -> Result<StateRevision, StorageError> {
    let attempt_id = current
        .attempt_id
        .ok_or(StorageError::StaleDownloadAttempt)?;
    match finalization {
        Finalization::Succeeded {
            artifact_key,
            byte_count,
            digest,
        } => apply_success(
            transaction,
            current,
            request_id,
            attempt_id,
            &artifact_key,
            byte_count,
            digest,
            now_ms,
        )?,
        Finalization::InvalidArtifact => {
            apply_invalid(transaction, current, request_id, attempt_id, now_ms)?
        }
    }
    let updated = crate::download_store_read::workflow(transaction, current.episode_id)?
        .ok_or(StorageError::DownloadWorkflowNotFound)?;
    if updated.workflow_revision.value != current.workflow_revision.value.saturating_add(1) {
        return Err(StorageError::RevisionConflict);
    }
    Ok(updated.workflow_revision)
}

#[allow(clippy::too_many_arguments)]
fn apply_success(
    transaction: &rusqlite::Transaction<'_>,
    current: &DownloadWorkflowRecord,
    request_id: HostRequestId,
    attempt_id: DownloadAttemptId,
    artifact_key: &str,
    byte_count: u64,
    digest: [u8; 32],
    now_ms: i64,
) -> Result<(), StorageError> {
    let byte_count =
        i64::try_from(byte_count).map_err(|_| StorageError::InvalidDownloadArtifact)?;
    let changed = transaction
        .execute(
            "UPDATE pod0_download_attempts SET state='succeeded',staged_path=NULL,\
         staged_byte_count=NULL,staged_digest=NULL,updated_at_ms=?1 \
         WHERE attempt_id=?2 AND request_id=?3 AND state='transferring'",
            params![
                now_ms,
                attempt_id.into_bytes().as_slice(),
                request_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("finalize download attempt", error))?;
    if changed != 1 {
        return Err(StorageError::StaleDownloadAttempt);
    }
    transaction.execute(
        "UPDATE pod0_episodes SET download_code=2,download_wire_code=NULL,download_ref_version=1,\
         download_ref_key=?1,download_byte_count=?2 WHERE episode_id=?3",
        params![artifact_key,byte_count,current.episode_id.into_bytes().as_slice()],
    ).map_err(|error| StorageError::sqlite("adopt finalized download",error))?;
    let changed = transaction.execute(
        "UPDATE pod0_download_workflows SET stage='succeeded',workflow_revision=workflow_revision+1,\
         request_id=NULL,deadline_at_ms=NULL,not_before_ms=NULL,artifact_key=?1,\
         artifact_byte_count=?2,artifact_digest=?3,failure_code=NULL,failure_detail=NULL,\
         failure_retryable=0,updated_at_ms=?4 WHERE episode_id=?5 AND request_id=?6 \
         AND attempt_id=?7 AND stage='transferring'",
        params![artifact_key,byte_count,digest.as_slice(),now_ms,current.episode_id.into_bytes().as_slice(),
            request_id.into_bytes().as_slice(),attempt_id.into_bytes().as_slice()],
    ).map_err(|error| StorageError::sqlite("complete finalized download",error))?;
    if changed != 1 {
        return Err(StorageError::StaleDownloadAttempt);
    }
    Ok(())
}

fn apply_invalid(
    transaction: &rusqlite::Transaction<'_>,
    current: &DownloadWorkflowRecord,
    request_id: HostRequestId,
    attempt_id: DownloadAttemptId,
    now_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_download_attempts SET state='failed',failure_code='invalid_artifact',\
         failure_detail=NULL,updated_at_ms=?1 WHERE attempt_id=?2 AND request_id=?3 \
         AND state='transferring'",
            params![
                now_ms,
                attempt_id.into_bytes().as_slice(),
                request_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("reject finalized download", error))?;
    let changed = transaction.execute(
        "UPDATE pod0_download_workflows SET stage='failed',workflow_revision=workflow_revision+1,\
         request_id=NULL,deadline_at_ms=NULL,not_before_ms=NULL,failure_code='invalid_artifact',\
         failure_detail=NULL,failure_retryable=0,updated_at_ms=?1 WHERE episode_id=?2 \
         AND request_id=?3 AND attempt_id=?4 AND stage='transferring'",
        params![now_ms,current.episode_id.into_bytes().as_slice(),request_id.into_bytes().as_slice(),
            attempt_id.into_bytes().as_slice()],
    ).map_err(|error| StorageError::sqlite("fail invalid finalized download",error))?;
    if changed != 1 {
        return Err(StorageError::StaleDownloadAttempt);
    }
    Ok(())
}
