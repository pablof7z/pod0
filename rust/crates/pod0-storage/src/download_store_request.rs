use pod0_domain::{
    CancellationId, CommandId, DownloadAttemptId, DownloadIntentId, EpisodeId, HostRequestId,
    StateRevision,
};
use rusqlite::{Transaction, params};
use sha2::{Digest as _, Sha256};

use crate::StorageError;
use crate::library_store::command_was_applied;

pub(crate) fn download_command_was_applied(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
) -> Result<Option<StateRevision>, StorageError> {
    command_was_applied(transaction, command_id, fingerprint).map_err(|error| match error {
        StorageError::CommandConflict => StorageError::DownloadCommandConflict,
        other => other,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_attempt_and_start_request(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
    intent_id: DownloadIntentId,
    attempt: u16,
    attempt_id: DownloadAttemptId,
    request_id: HostRequestId,
    command_id: CommandId,
    cancellation_id: CancellationId,
    issued_revision: StateRevision,
    deadline_at_ms: i64,
    input_version: &str,
    enclosure_url: &str,
    resume_key: Option<&str>,
    now_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO pod0_download_attempts(attempt_id,episode_id,intent_id,attempt,state,\
             request_id,external_task_key,resume_key,staged_path,staged_byte_count,staged_digest,\
             failure_code,failure_detail,created_at_ms,updated_at_ms) VALUES(?1,?2,?3,?4,\
             'requested',?5,NULL,?6,NULL,NULL,NULL,NULL,NULL,?7,?7)",
            params![
                attempt_id.into_bytes().as_slice(),
                episode_id.into_bytes().as_slice(),
                intent_id.into_bytes().as_slice(),
                i64::from(attempt),
                request_id.into_bytes().as_slice(),
                resume_key,
                now_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("insert download attempt", error))?;
    transaction
        .execute(
            "INSERT INTO pod0_download_host_requests(request_id,episode_id,kind,state,command_id,\
             cancellation_id,issued_revision,deadline_at_ms,intent_id,attempt_id,input_version,\
             enclosure_url,resume_key,external_task_key,artifact_key,last_sequence_number,\
             created_at_ms,updated_at_ms) VALUES(?1,?2,'start','pending',?3,?4,?5,?6,?7,?8,\
             ?9,?10,?11,NULL,NULL,NULL,?12,?12)",
            params![
                request_id.into_bytes().as_slice(),
                episode_id.into_bytes().as_slice(),
                command_id.into_bytes().as_slice(),
                cancellation_id.into_bytes().as_slice(),
                u64_to_i64(issued_revision.value)?,
                deadline_at_ms,
                intent_id.into_bytes().as_slice(),
                attempt_id.into_bytes().as_slice(),
                input_version,
                enclosure_url,
                resume_key,
                now_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("insert download start request", error))?;
    Ok(())
}

pub fn download_start_request_id(attempt_id: DownloadAttemptId) -> HostRequestId {
    derived_request_id(
        b"pod0-download-start-request-v1",
        &attempt_id.into_bytes(),
        0,
    )
}

pub(crate) fn derived_request_id(domain: &[u8], identity: &[u8], revision: u64) -> HostRequestId {
    let mut hash = Sha256::new();
    hash.update((domain.len() as u64).to_be_bytes());
    hash.update(domain);
    hash.update((identity.len() as u64).to_be_bytes());
    hash.update(identity);
    hash.update(revision.to_be_bytes());
    HostRequestId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}

pub(crate) fn retire_request(
    transaction: &Transaction<'_>,
    request_id: Option<HostRequestId>,
    now_ms: i64,
) -> Result<(), StorageError> {
    if let Some(request_id) = request_id {
        transaction
            .execute(
                "UPDATE pod0_download_host_requests SET state='retired',updated_at_ms=?1 \
                 WHERE request_id=?2 AND state='pending'",
                params![now_ms, request_id.into_bytes().as_slice()],
            )
            .map_err(|error| StorageError::sqlite("retire download request", error))?;
    }
    Ok(())
}

pub(crate) fn u64_to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::DownloadWorkflowConflict)
}
