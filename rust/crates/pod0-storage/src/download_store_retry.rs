use pod0_domain::download_attempt_identity;
use rusqlite::params;

use crate::download_store_read::workflow_for_request;
use crate::download_store_request::{
    insert_attempt_and_start_request, start_request_id, u64_to_i64,
};
use crate::{
    DownloadFailureInput, DownloadObservationOutcome, DownloadWorkflowRecord, StorageError,
};

pub(crate) fn schedule_retry(
    transaction: &rusqlite::Transaction<'_>,
    current: DownloadWorkflowRecord,
    input: DownloadFailureInput,
) -> Result<DownloadObservationOutcome, StorageError> {
    let attempt = current
        .attempt
        .checked_add(1)
        .ok_or(StorageError::DownloadWorkflowConflict)?;
    let attempt_id = download_attempt_identity(current.intent_id, attempt)
        .ok_or(StorageError::DownloadWorkflowConflict)?;
    let request_id = start_request_id(attempt_id);
    let retry_at = input
        .retry_at_ms
        .ok_or(StorageError::DownloadWorkflowConflict)?;
    let deadline = input
        .retry_deadline_at_ms
        .ok_or(StorageError::DownloadWorkflowConflict)?;
    insert_attempt_and_start_request(
        transaction,
        current.episode_id,
        current.intent_id,
        attempt,
        attempt_id,
        request_id,
        current.command_id,
        current.cancellation_id,
        input.issued_revision,
        deadline,
        &current.input_version,
        &current.enclosure_url,
        current.resume_key.as_deref(),
        input.observed_at_ms,
    )?;
    transaction
        .execute(
            "UPDATE pod0_download_workflows SET stage='retry_scheduled',\
             workflow_revision=workflow_revision+1,attempt=?1,attempt_id=?2,request_id=?3,\
             issued_revision=?4,deadline_at_ms=?5,not_before_ms=?6,failure_code=?7,\
             failure_detail=?8,failure_retryable=1,updated_at_ms=?9 WHERE episode_id=?10 \
             AND request_id=?11",
            params![
                i64::from(attempt),
                attempt_id.into_bytes().as_slice(),
                request_id.into_bytes().as_slice(),
                u64_to_i64(input.issued_revision.value)?,
                deadline,
                retry_at,
                input.failure_code,
                input.failure_detail,
                input.observed_at_ms,
                current.episode_id.into_bytes().as_slice(),
                input.request_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("schedule download retry", error))?;
    if transaction.changes() != 1 {
        return Err(StorageError::StaleDownloadAttempt);
    }
    Ok(DownloadObservationOutcome::Updated(workflow_for_request(
        transaction,
        request_id,
    )?))
}
