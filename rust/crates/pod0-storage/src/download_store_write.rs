use pod0_domain::{
    CommandId, DownloadIntentId, EpisodeId, StateRevision, download_attempt_identity,
};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::download_store_read::workflow;
use crate::download_store_request::{
    download_command_was_applied, insert_attempt_and_start_request, retire_request,
    start_request_id, u64_to_i64,
};
use crate::library_store::finish_command;
use crate::{
    DownloadEnsureInput, DownloadEnsureOutcome, DownloadWorkflowRecord, LibraryStore, StorageError,
    StoredDownloadNetwork, StoredDownloadStage,
};

impl LibraryStore {
    pub fn ensure_download_workflow(
        &self,
        input: DownloadEnsureInput,
    ) -> Result<DownloadEnsureOutcome, StorageError> {
        self.write(|transaction| {
            if download_command_was_applied(
                transaction,
                input.command_id,
                &input.command_fingerprint,
            )?
            .is_some()
            {
                return workflow(transaction, input.episode_id)?
                    .map(DownloadEnsureOutcome::Existing)
                    .ok_or(StorageError::DownloadWorkflowNotFound);
            }
            require_current_enclosure(transaction, input.episode_id, &input.enclosure_url)?;
            let existing = workflow(transaction, input.episode_id)?;
            if existing.as_ref().is_some_and(|record| {
                record.intent_id == input.intent_id
                    && (record.stage.is_active()
                        || record.stage == StoredDownloadStage::Succeeded
                        || (!input.admitted && record.stage == StoredDownloadStage::Waiting))
            }) {
                finish_command(
                    transaction,
                    input.command_id,
                    &input.command_fingerprint,
                    input.now_ms,
                )?;
                return Ok(DownloadEnsureOutcome::Existing(
                    existing.expect("checked existing workflow"),
                ));
            }
            if let Some(record) = existing.as_ref() {
                retire_request(transaction, record.request_id, input.now_ms)?;
            }
            let revision = next_workflow_revision(existing.as_ref())?;
            let attempt = if input.admitted {
                next_attempt(existing.as_ref(), input.intent_id)?
            } else {
                existing
                    .as_ref()
                    .filter(|record| record.intent_id == input.intent_id)
                    .map_or(0, |record| record.attempt)
            };
            let attempt_id = if input.admitted {
                download_attempt_identity(input.intent_id, attempt)
            } else {
                existing
                    .as_ref()
                    .filter(|record| record.intent_id == input.intent_id)
                    .and_then(|record| record.attempt_id)
            };
            let request_id = input.admitted.then(|| attempt_id.map(start_request_id)).flatten();
            let stage = if input.admitted {
                StoredDownloadStage::Requested
            } else {
                StoredDownloadStage::Waiting
            };
            let created_at = existing
                .as_ref()
                .map_or(input.now_ms, |record| record.created_at_ms);
            let (origin_code, origin_wire) = input.origin.wire();
            transaction
                .execute(
                    "INSERT INTO pod0_download_workflows(episode_id,intent_id,input_version,\
                     origin_code,origin_wire_code,desired_state,stage,workflow_revision,attempt,\
                     attempt_id,request_id,command_id,cancellation_id,issued_revision,deadline_at_ms,\
                     not_before_ms,enclosure_url,resume_key,external_task_key,artifact_key,\
                     artifact_byte_count,artifact_digest,failure_code,failure_detail,\
                     failure_retryable,created_at_ms,updated_at_ms) VALUES(?1,?2,?3,?4,?5,\
                     'present',?6,?7,?8,?9,?10,?11,?12,?13,?14,NULL,?15,NULL,NULL,NULL,NULL,\
                     NULL,?16,NULL,0,?17,?18) ON CONFLICT(episode_id) DO UPDATE SET \
                     intent_id=excluded.intent_id,input_version=excluded.input_version,\
                     origin_code=excluded.origin_code,origin_wire_code=excluded.origin_wire_code,\
                     desired_state='present',stage=excluded.stage,\
                     workflow_revision=excluded.workflow_revision,attempt=excluded.attempt,\
                     attempt_id=excluded.attempt_id,request_id=excluded.request_id,\
                     command_id=excluded.command_id,cancellation_id=excluded.cancellation_id,\
                     issued_revision=excluded.issued_revision,deadline_at_ms=excluded.deadline_at_ms,\
                     not_before_ms=NULL,enclosure_url=excluded.enclosure_url,resume_key=NULL,\
                     external_task_key=NULL,artifact_key=NULL,artifact_byte_count=NULL,\
                     artifact_digest=NULL,failure_code=excluded.failure_code,failure_detail=NULL,\
                     failure_retryable=0,updated_at_ms=excluded.updated_at_ms",
                    params![
                        input.episode_id.into_bytes().as_slice(),
                        input.intent_id.into_bytes().as_slice(),
                        input.input_version,
                        origin_code,
                        origin_wire,
                        stage.wire(),
                        u64_to_i64(revision.value)?,
                        i64::from(attempt),
                        attempt_id.map(|id| id.into_bytes().to_vec()),
                        request_id.map(|id| id.into_bytes().to_vec()),
                        input.command_id.into_bytes().as_slice(),
                        input.cancellation_id.into_bytes().as_slice(),
                        u64_to_i64(input.issued_revision.value)?,
                        input.admitted.then_some(input.deadline_at_ms),
                        input.enclosure_url,
                        input.wait_failure_code,
                        created_at,
                        input.now_ms,
                    ],
                )
                .map_err(|error| StorageError::sqlite("ensure download workflow", error))?;
            if let (Some(attempt_id), Some(request_id)) = (attempt_id, request_id) {
                insert_attempt_and_start_request(
                    transaction,
                    input.episode_id,
                    input.intent_id,
                    attempt,
                    attempt_id,
                    request_id,
                    input.command_id,
                    input.cancellation_id,
                    input.issued_revision,
                    input.deadline_at_ms,
                    &input.input_version,
                    &input.enclosure_url,
                    None,
                    input.now_ms,
                )?;
            }
            finish_command(
                transaction,
                input.command_id,
                &input.command_fingerprint,
                input.now_ms,
            )?;
            let record = workflow(transaction, input.episode_id)?
                .ok_or(StorageError::DownloadWorkflowNotFound)?;
            Ok(DownloadEnsureOutcome::Changed {
                record,
                replaced: existing.map(Box::new),
            })
        })
    }

    pub fn observe_download_environment(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        network: StoredDownloadNetwork,
        available_capacity_bytes: Option<u64>,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        self.write(|transaction| {
            if let Some(revision) =
                download_command_was_applied(transaction, command_id, command_fingerprint)?
            {
                return Ok(revision);
            }
            let (code, wire) = network.wire();
            transaction
                .execute(
                    "UPDATE pod0_download_environment SET network_code=?1,network_wire_code=?2,\
                     available_capacity_bytes=?3,observed_at_ms=?4 WHERE singleton=1",
                    params![
                        code,
                        wire,
                        available_capacity_bytes.map(u64_to_i64).transpose()?,
                        observed_at_ms
                    ],
                )
                .map_err(|error| StorageError::sqlite("update download environment", error))?;
            finish_command(transaction, command_id, command_fingerprint, observed_at_ms)
        })
    }

    pub fn record_download_noop_command(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        self.write(|transaction| {
            if let Some(revision) =
                download_command_was_applied(transaction, command_id, command_fingerprint)?
            {
                return Ok(revision);
            }
            finish_command(transaction, command_id, command_fingerprint, observed_at_ms)
        })
    }
}

fn require_current_enclosure(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
    enclosure_url: &str,
) -> Result<(), StorageError> {
    let stored: Option<String> = transaction
        .query_row(
            "SELECT enclosure_url FROM pod0_episodes WHERE episode_id=?1",
            [episode_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read download enclosure", error))?;
    match stored {
        None => Err(StorageError::EntityNotFound),
        Some(value) if value == enclosure_url => Ok(()),
        Some(_) => Err(StorageError::DownloadWorkflowConflict),
    }
}

fn next_workflow_revision(
    existing: Option<&DownloadWorkflowRecord>,
) -> Result<StateRevision, StorageError> {
    let value = existing.map_or(1, |record| {
        record.workflow_revision.value.checked_add(1).unwrap_or(0)
    });
    if value == 0 {
        Err(StorageError::DownloadWorkflowConflict)
    } else {
        Ok(StateRevision::new(value))
    }
}

fn next_attempt(
    existing: Option<&DownloadWorkflowRecord>,
    intent_id: DownloadIntentId,
) -> Result<u16, StorageError> {
    existing
        .filter(|record| record.intent_id == intent_id)
        .map_or(Ok(1), |record| {
            record
                .attempt
                .checked_add(1)
                .ok_or(StorageError::DownloadWorkflowConflict)
        })
}
