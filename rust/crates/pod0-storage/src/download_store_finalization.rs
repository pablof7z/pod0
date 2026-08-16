use pod0_domain::HostRequestId;
use rusqlite::params;

use crate::download_store_artifact::complete_request;
use crate::download_store_read::{request, workflow};
use crate::{DownloadHostRequestKind, DownloadObservationOutcome, StorageError};

pub(crate) fn apply_download_finalization_queued(
    transaction: &rusqlite::Transaction<'_>,
    request_id: HostRequestId,
    sequence_number: u64,
    observed_at_ms: i64,
) -> Result<DownloadObservationOutcome, StorageError> {
    let Some((host, state)) = request(transaction, request_id)? else {
        return Ok(DownloadObservationOutcome::Stale);
    };
    let current =
        workflow(transaction, host.episode_id)?.ok_or(StorageError::DownloadWorkflowNotFound)?;
    if state != "pending"
        || host
            .last_sequence_number
            .is_some_and(|value| value >= sequence_number)
    {
        return Ok(DownloadObservationOutcome::Duplicate(current));
    }
    if host.kind != DownloadHostRequestKind::Start
        || current.request_id != Some(request_id)
        || current.attempt_id != host.attempt_id
    {
        return Ok(DownloadObservationOutcome::Stale);
    }
    complete_request(transaction, request_id, sequence_number, observed_at_ms)?;
    let attempt_id = host.attempt_id.ok_or(StorageError::StaleDownloadAttempt)?;
    let changed = transaction
        .execute(
            "UPDATE pod0_download_attempts SET state='transferring',updated_at_ms=?1 \
             WHERE attempt_id=?2 AND request_id=?3 \
             AND state IN('requested','host_accepted','transferring')",
            params![
                observed_at_ms,
                attempt_id.into_bytes().as_slice(),
                request_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("queue download finalization", error))?;
    if changed != 1 {
        return Err(StorageError::StaleDownloadAttempt);
    }
    let changed = transaction
        .execute(
            "UPDATE pod0_download_workflows SET stage='transferring',\
             workflow_revision=workflow_revision+1,deadline_at_ms=NULL,updated_at_ms=?1 \
             WHERE episode_id=?2 AND request_id=?3 AND attempt_id=?4 \
             AND stage IN('requested','host_accepted','transferring')",
            params![
                observed_at_ms,
                current.episode_id.into_bytes().as_slice(),
                request_id.into_bytes().as_slice(),
                attempt_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("advance download finalization state", error))?;
    if changed != 1 {
        return Err(StorageError::StaleDownloadAttempt);
    }
    Ok(DownloadObservationOutcome::Updated(
        workflow(transaction, current.episode_id)?.ok_or(StorageError::DownloadWorkflowNotFound)?,
    ))
}
