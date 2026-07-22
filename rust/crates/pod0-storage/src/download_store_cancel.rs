use pod0_domain::{CommandId, EpisodeId, HostRequestId, StateRevision};
use rusqlite::params;

use crate::download_store_read::workflow;
use crate::download_store_request::{
    derived_request_id, download_command_was_applied, retire_request, u64_to_i64,
};
use crate::library_store::finish_command;
use crate::{
    DownloadRemovalInput, DownloadWorkflowRecord, DownloadWorkflowTransition, LibraryStore,
    StorageError, StoredDownloadStage,
};

impl LibraryStore {
    pub fn cancel_download_workflow(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        episode_id: EpisodeId,
        expected_revision: StateRevision,
        issued_revision: StateRevision,
        now_ms: i64,
    ) -> Result<DownloadWorkflowTransition, StorageError> {
        self.write(|transaction| {
            if download_command_was_applied(transaction, command_id, command_fingerprint)?.is_some()
            {
                let record = workflow(transaction, episode_id)?
                    .ok_or(StorageError::DownloadWorkflowNotFound)?;
                return Ok(DownloadWorkflowTransition {
                    record,
                    replaced: None,
                });
            }
            let existing =
                workflow(transaction, episode_id)?.ok_or(StorageError::DownloadWorkflowNotFound)?;
            if existing.workflow_revision != expected_revision
                || matches!(
                    existing.stage,
                    StoredDownloadStage::Succeeded | StoredDownloadStage::Removing
                )
            {
                return Err(StorageError::DownloadWorkflowConflict);
            }
            retire_request(transaction, existing.request_id, now_ms)?;
            if let Some(attempt_id) = existing.attempt_id {
                let request_id = derived_request_id(
                    b"pod0-download-cancel-request-v1",
                    &attempt_id.into_bytes(),
                    expected_revision.value,
                );
                insert_cancel_request(
                    transaction,
                    &existing,
                    request_id,
                    command_id,
                    issued_revision,
                    now_ms,
                )?;
            }
            transaction
                .execute(
                    "UPDATE pod0_download_workflows SET desired_state='absent',stage='cancelled',\
                     workflow_revision=workflow_revision+1,request_id=NULL,deadline_at_ms=NULL,\
                     not_before_ms=NULL,failure_code=NULL,failure_detail=NULL,failure_retryable=0,\
                     updated_at_ms=?1 WHERE episode_id=?2 AND workflow_revision=?3",
                    params![
                        now_ms,
                        episode_id.into_bytes().as_slice(),
                        u64_to_i64(expected_revision.value)?
                    ],
                )
                .map_err(|error| StorageError::sqlite("cancel download workflow", error))?;
            if transaction.changes() != 1 {
                return Err(StorageError::DownloadWorkflowConflict);
            }
            finish_command(transaction, command_id, command_fingerprint, now_ms)?;
            let record =
                workflow(transaction, episode_id)?.ok_or(StorageError::DownloadWorkflowNotFound)?;
            Ok(DownloadWorkflowTransition {
                record,
                replaced: Some(Box::new(existing)),
            })
        })
    }

    pub fn remove_download_artifact(
        &self,
        input: DownloadRemovalInput,
    ) -> Result<DownloadWorkflowTransition, StorageError> {
        self.write(|transaction| {
            if download_command_was_applied(
                transaction,
                input.command_id,
                &input.command_fingerprint,
            )?
            .is_some()
            {
                let record = workflow(transaction, input.episode_id)?
                    .ok_or(StorageError::DownloadWorkflowNotFound)?;
                return Ok(DownloadWorkflowTransition {
                    record,
                    replaced: None,
                });
            }
            let existing = workflow(transaction, input.episode_id)?
                .ok_or(StorageError::DownloadWorkflowNotFound)?;
            if existing.workflow_revision != input.expected_revision
                || !matches!(
                    existing.stage,
                    StoredDownloadStage::Succeeded | StoredDownloadStage::Failed
                )
            {
                return Err(StorageError::DownloadWorkflowConflict);
            }
            let artifact_key = existing
                .artifact_key
                .as_deref()
                .ok_or(StorageError::InvalidDownloadArtifact)?;
            let request_id = derived_request_id(
                b"pod0-download-remove-request-v1",
                artifact_key.as_bytes(),
                input.expected_revision.value,
            );
            insert_remove_request(
                transaction,
                &existing,
                request_id,
                input.command_id,
                input.issued_revision,
                artifact_key,
                input.deadline_at_ms,
                input.now_ms,
            )?;
            transaction
                .execute(
                    "UPDATE pod0_download_workflows SET desired_state='absent',stage='removing',\
                     workflow_revision=workflow_revision+1,request_id=?1,command_id=?2,\
                     issued_revision=?3,deadline_at_ms=?4,not_before_ms=NULL,failure_code=NULL,\
                     failure_detail=NULL,failure_retryable=0,updated_at_ms=?5 WHERE episode_id=?6 \
                     AND workflow_revision=?7 AND stage IN('succeeded','failed')",
                    params![
                        request_id.into_bytes().as_slice(),
                        input.command_id.into_bytes().as_slice(),
                        u64_to_i64(input.issued_revision.value)?,
                        input.deadline_at_ms,
                        input.now_ms,
                        input.episode_id.into_bytes().as_slice(),
                        u64_to_i64(input.expected_revision.value)?
                    ],
                )
                .map_err(|error| StorageError::sqlite("request download removal", error))?;
            if transaction.changes() != 1 {
                return Err(StorageError::DownloadWorkflowConflict);
            }
            finish_command(
                transaction,
                input.command_id,
                &input.command_fingerprint,
                input.now_ms,
            )?;
            let record = workflow(transaction, input.episode_id)?
                .ok_or(StorageError::DownloadWorkflowNotFound)?;
            Ok(DownloadWorkflowTransition {
                record,
                replaced: Some(Box::new(existing)),
            })
        })
    }
}

fn insert_cancel_request(
    transaction: &rusqlite::Transaction<'_>,
    record: &DownloadWorkflowRecord,
    request_id: HostRequestId,
    command_id: CommandId,
    issued_revision: StateRevision,
    now_ms: i64,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO pod0_download_host_requests(request_id,episode_id,kind,state,command_id,\
         cancellation_id,issued_revision,deadline_at_ms,intent_id,attempt_id,input_version,\
         enclosure_url,resume_key,external_task_key,artifact_key,last_sequence_number,created_at_ms,\
         updated_at_ms) VALUES(?1,?2,'cancel','pending',?3,?4,?5,NULL,?6,?7,NULL,NULL,NULL,?8,\
         NULL,NULL,?9,?9)",
        params![request_id.into_bytes().as_slice(),record.episode_id.into_bytes().as_slice(),
            command_id.into_bytes().as_slice(),record.cancellation_id.into_bytes().as_slice(),
            u64_to_i64(issued_revision.value)?,record.intent_id.into_bytes().as_slice(),
            record.attempt_id.map(|id| id.into_bytes().to_vec()),record.external_task_key,now_ms],
    ).map_err(|error| StorageError::sqlite("insert download cancel request", error))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_remove_request(
    transaction: &rusqlite::Transaction<'_>,
    record: &DownloadWorkflowRecord,
    request_id: HostRequestId,
    command_id: CommandId,
    issued_revision: StateRevision,
    artifact_key: &str,
    deadline_at_ms: i64,
    now_ms: i64,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO pod0_download_host_requests(request_id,episode_id,kind,state,command_id,\
         cancellation_id,issued_revision,deadline_at_ms,intent_id,attempt_id,input_version,\
         enclosure_url,resume_key,external_task_key,artifact_key,last_sequence_number,created_at_ms,\
         updated_at_ms) VALUES(?1,?2,'remove','pending',?3,?4,?5,?6,NULL,NULL,NULL,NULL,NULL,NULL,\
         ?7,NULL,?8,?8)",
        params![request_id.into_bytes().as_slice(),record.episode_id.into_bytes().as_slice(),
            command_id.into_bytes().as_slice(),record.cancellation_id.into_bytes().as_slice(),
            u64_to_i64(issued_revision.value)?,deadline_at_ms,artifact_key,now_ms],
    ).map_err(|error| StorageError::sqlite("insert download remove request", error))?;
    Ok(())
}
