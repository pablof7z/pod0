use pod0_domain::HostRequestId;
use rusqlite::params;

use crate::download_store_artifact::complete_request;
use crate::download_store_read::{request, workflow};
use crate::download_store_request::u64_to_i64;
use crate::download_store_retry::schedule_retry;
use crate::{
    DownloadFailureInput, DownloadHostRequestKind, DownloadObservationOutcome, LibraryStore,
    StorageError,
};

impl LibraryStore {
    pub fn accept_download_host_task(
        &self,
        request_id: HostRequestId,
        sequence_number: u64,
        external_task_key: &str,
        resume_key: Option<&str>,
        observed_at_ms: i64,
    ) -> Result<DownloadObservationOutcome, StorageError> {
        self.write(|transaction| {
            let Some((host, state)) = request(transaction, request_id)? else {
                return Ok(DownloadObservationOutcome::Stale);
            };
            let current = workflow(transaction, host.episode_id)?
                .ok_or(StorageError::DownloadWorkflowNotFound)?;
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
            let changed = transaction
                .execute(
                    "UPDATE pod0_download_host_requests SET external_task_key=?1,resume_key=?2,\
                 last_sequence_number=?3,updated_at_ms=?4 WHERE request_id=?5 AND state='pending' \
                 AND (last_sequence_number IS NULL OR last_sequence_number<?3)",
                    params![
                        external_task_key,
                        resume_key,
                        u64_to_i64(sequence_number)?,
                        observed_at_ms,
                        request_id.into_bytes().as_slice()
                    ],
                )
                .map_err(|error| StorageError::sqlite("accept download host task", error))?;
            if changed != 1 {
                return Ok(DownloadObservationOutcome::Duplicate(current));
            }
            transaction
                .execute(
                    "UPDATE pod0_download_attempts SET state='host_accepted',external_task_key=?1,\
                 resume_key=?2,updated_at_ms=?3 WHERE attempt_id=?4 AND request_id=?5 \
                 AND state IN('requested','host_accepted')",
                    params![
                        external_task_key,
                        resume_key,
                        observed_at_ms,
                        host.attempt_id.map(|id| id.into_bytes().to_vec()),
                        request_id.into_bytes().as_slice()
                    ],
                )
                .map_err(|error| StorageError::sqlite("accept download attempt", error))?;
            transaction
                .execute(
                    "UPDATE pod0_download_workflows SET stage='host_accepted',\
                 workflow_revision=workflow_revision+1,external_task_key=?1,resume_key=?2,\
                 updated_at_ms=?3 WHERE episode_id=?4 AND request_id=?5 AND attempt_id=?6",
                    params![
                        external_task_key,
                        resume_key,
                        observed_at_ms,
                        current.episode_id.into_bytes().as_slice(),
                        request_id.into_bytes().as_slice(),
                        host.attempt_id.map(|id| id.into_bytes().to_vec())
                    ],
                )
                .map_err(|error| StorageError::sqlite("accept download workflow", error))?;
            if transaction.changes() != 1 {
                return Err(StorageError::StaleDownloadAttempt);
            }
            Ok(DownloadObservationOutcome::Updated(
                workflow(transaction, current.episode_id)?
                    .ok_or(StorageError::DownloadWorkflowNotFound)?,
            ))
        })
    }

    pub fn complete_download_cancellation(
        &self,
        request_id: HostRequestId,
        sequence_number: u64,
        observed_at_ms: i64,
    ) -> Result<DownloadObservationOutcome, StorageError> {
        self.write(|transaction| {
            let Some((host,state))=request(transaction,request_id)? else {return Ok(DownloadObservationOutcome::Stale)};
            let current=workflow(transaction,host.episode_id)?.ok_or(StorageError::DownloadWorkflowNotFound)?;
            if state!="pending"||host.last_sequence_number.is_some_and(|value|value>=sequence_number){
                return Ok(DownloadObservationOutcome::Duplicate(current));
            }
            if !matches!(host.kind,DownloadHostRequestKind::Start|DownloadHostRequestKind::Cancel){
                return Ok(DownloadObservationOutcome::Stale);
            }
            complete_request(transaction,request_id,sequence_number,observed_at_ms)?;
            if let Some(attempt_id)=host.attempt_id{
                transaction.execute(
                    "UPDATE pod0_download_attempts SET state='cancelled',updated_at_ms=?1 \
                     WHERE attempt_id=?2 AND state NOT IN('succeeded','failed')",
                    params![observed_at_ms,attempt_id.into_bytes().as_slice()],
                ).map_err(|error|StorageError::sqlite("cancel download attempt",error))?;
            }
            if current.request_id==Some(request_id){
                transaction.execute(
                    "UPDATE pod0_download_workflows SET desired_state='absent',stage='cancelled',\
                     workflow_revision=workflow_revision+1,request_id=NULL,deadline_at_ms=NULL,\
                     not_before_ms=NULL,failure_code=NULL,failure_detail=NULL,failure_retryable=0,\
                     updated_at_ms=?1 WHERE episode_id=?2 AND request_id=?3",
                    params![observed_at_ms,current.episode_id.into_bytes().as_slice(),request_id.into_bytes().as_slice()],
                ).map_err(|error|StorageError::sqlite("cancel download workflow observation",error))?;
            }
            Ok(DownloadObservationOutcome::Updated(
                workflow(transaction,current.episode_id)?.ok_or(StorageError::DownloadWorkflowNotFound)?
            ))
        })
    }

    pub fn complete_download_artifact_removal(
        &self,
        request_id: HostRequestId,
        sequence_number: u64,
        artifact_key: &str,
        observed_at_ms: i64,
    ) -> Result<DownloadObservationOutcome, StorageError> {
        self.write(|transaction| {
            let Some((host,state))=request(transaction,request_id)? else {return Ok(DownloadObservationOutcome::Stale)};
            let current=workflow(transaction,host.episode_id)?.ok_or(StorageError::DownloadWorkflowNotFound)?;
            if state!="pending"||host.last_sequence_number.is_some_and(|value|value>=sequence_number){
                return Ok(DownloadObservationOutcome::Duplicate(current));
            }
            if host.kind!=DownloadHostRequestKind::Remove||host.artifact_key.as_deref()!=Some(artifact_key)
                ||current.request_id!=Some(request_id){return Ok(DownloadObservationOutcome::Stale)}
            complete_request(transaction,request_id,sequence_number,observed_at_ms)?;
            transaction.execute(
                "UPDATE pod0_episodes SET download_code=1,download_wire_code=NULL,\
                 download_ref_version=NULL,download_ref_key=NULL,download_byte_count=NULL WHERE episode_id=?1",
                [current.episode_id.into_bytes().as_slice()],
            ).map_err(|error|StorageError::sqlite("remove episode download artifact",error))?;
            transaction.execute(
                "UPDATE pod0_download_workflows SET stage='cancelled',\
                 workflow_revision=workflow_revision+1,request_id=NULL,deadline_at_ms=NULL,\
                 artifact_key=NULL,artifact_byte_count=NULL,artifact_digest=NULL,failure_code=NULL,\
                 failure_detail=NULL,failure_retryable=0,updated_at_ms=?1 WHERE episode_id=?2 \
                 AND request_id=?3 AND stage='removing'",
                params![observed_at_ms,current.episode_id.into_bytes().as_slice(),request_id.into_bytes().as_slice()],
            ).map_err(|error|StorageError::sqlite("complete download artifact removal",error))?;
            if transaction.changes()!=1{return Err(StorageError::DownloadWorkflowConflict)}
            Ok(DownloadObservationOutcome::Updated(
                workflow(transaction,current.episode_id)?.ok_or(StorageError::DownloadWorkflowNotFound)?
            ))
        })
    }

    pub fn fail_download_host_request(
        &self,
        input: DownloadFailureInput,
    ) -> Result<DownloadObservationOutcome, StorageError> {
        self.write(|transaction| {
            let Some((host, state)) = request(transaction, input.request_id)? else {
                return Ok(DownloadObservationOutcome::Stale);
            };
            let current = workflow(transaction, host.episode_id)?
                .ok_or(StorageError::DownloadWorkflowNotFound)?;
            if state != "pending"
                || host
                    .last_sequence_number
                    .is_some_and(|value| value >= input.sequence_number)
            {
                return Ok(DownloadObservationOutcome::Duplicate(current));
            }
            complete_request(
                transaction,
                input.request_id,
                input.sequence_number,
                input.observed_at_ms,
            )?;
            if let Some(attempt_id) = host.attempt_id {
                transaction
                    .execute(
                        "UPDATE pod0_download_attempts SET state='failed',failure_code=?1,\
                     failure_detail=?2,staged_path=NULL,staged_byte_count=NULL,staged_digest=NULL,\
                     updated_at_ms=?3 WHERE attempt_id=?4 AND state!='succeeded'",
                        params![
                            input.failure_code,
                            input.failure_detail,
                            input.observed_at_ms,
                            attempt_id.into_bytes().as_slice()
                        ],
                    )
                    .map_err(|error| StorageError::sqlite("fail download attempt", error))?;
            }
            let retry = host.kind == DownloadHostRequestKind::Start
                && input.retryable
                && input.retry_at_ms.is_some()
                && current.attempt < u16::MAX;
            if retry {
                return schedule_retry(transaction, current, input);
            }
            if current.request_id == Some(input.request_id) {
                transaction
                    .execute(
                        "UPDATE pod0_download_workflows SET stage='failed',\
                     workflow_revision=workflow_revision+1,request_id=NULL,deadline_at_ms=NULL,\
                     not_before_ms=NULL,failure_code=?1,failure_detail=?2,failure_retryable=?3,\
                     updated_at_ms=?4 WHERE episode_id=?5 AND request_id=?6",
                        params![
                            input.failure_code,
                            input.failure_detail,
                            i64::from(input.retryable),
                            input.observed_at_ms,
                            current.episode_id.into_bytes().as_slice(),
                            input.request_id.into_bytes().as_slice()
                        ],
                    )
                    .map_err(|error| StorageError::sqlite("fail download workflow", error))?;
            }
            Ok(DownloadObservationOutcome::Updated(
                workflow(transaction, current.episode_id)?
                    .ok_or(StorageError::DownloadWorkflowNotFound)?,
            ))
        })
    }
}
