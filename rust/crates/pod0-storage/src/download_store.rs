use pod0_domain::{EpisodeId, HostRequestId, StateRevision, download_attempt_identity};
use rusqlite::{OptionalExtension, params};

use crate::download_store_read::{environment, page, pending_requests, request, workflow};
use crate::download_store_request::{
    download_start_request_id, insert_attempt_and_start_request, u64_to_i64,
};
use crate::{
    DownloadEnvironmentRecord, DownloadHostRequestRecord, DownloadWorkflowPage,
    DownloadWorkflowRecord, DownloadWorkflowTransition, LibraryStore, StorageError,
    StoredDownloadStage,
};

impl LibraryStore {
    pub fn download_environment(&self) -> Result<DownloadEnvironmentRecord, StorageError> {
        self.read(environment)
    }

    pub fn download_workflow(
        &self,
        episode_id: EpisodeId,
    ) -> Result<Option<DownloadWorkflowRecord>, StorageError> {
        self.read(|connection| workflow(connection, episode_id))
    }

    pub fn download_workflow_page(
        &self,
        episode_id: Option<EpisodeId>,
        offset: u32,
        max_items: u16,
    ) -> Result<DownloadWorkflowPage, StorageError> {
        self.read(|connection| page(connection, episode_id, offset, max_items))
    }

    pub fn pending_download_host_requests(
        &self,
        max_items: u16,
    ) -> Result<Vec<DownloadHostRequestRecord>, StorageError> {
        if !self.download_workflow_authority()?.is_authoritative() {
            return Ok(Vec::new());
        }
        self.read(|connection| pending_requests(connection, max_items))
    }

    pub fn download_host_request(
        &self,
        request_id: HostRequestId,
    ) -> Result<Option<(DownloadHostRequestRecord, String)>, StorageError> {
        self.read(|connection| request(connection, request_id))
    }

    pub fn download_host_request_for_effect_intent(
        &self,
        intent_id: pod0_domain::EffectIntentId,
    ) -> Result<Option<DownloadHostRequestRecord>, StorageError> {
        self.read(|connection| {
            let owner: Option<(Vec<u8>, Option<Vec<u8>>)> = connection
                .query_row(
                    "SELECT i.episode_id,f.command_id FROM pod0_effect_intents i \
                     JOIN pod0_activity_facts f ON f.activity_id=i.authorizing_activity_id \
                     WHERE i.intent_id=?1 AND json_extract(i.request_json,'$.kind')='Download'",
                    [intent_id.into_bytes().as_slice()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|error| StorageError::sqlite("read download effect owner", error))?;
            let Some((episode, command)) = owner else {
                return Ok(None);
            };
            let request_id: Option<Vec<u8>> = if let Some(command) = command {
                connection
                    .query_row(
                        "SELECT request_id FROM pod0_download_host_requests WHERE episode_id=?1 \
                         AND command_id=?2 AND state='pending' ORDER BY created_at_ms DESC LIMIT 1",
                        params![episode, command],
                        |row| row.get(0),
                    )
                    .optional()
            } else {
                connection
                    .query_row(
                        "SELECT request_id FROM pod0_download_host_requests WHERE episode_id=?1 \
                         AND state='pending' ORDER BY created_at_ms DESC LIMIT 1",
                        [episode],
                        |row| row.get(0),
                    )
                    .optional()
            }
            .map_err(|error| StorageError::sqlite("read download effect request", error))?;
            request_id
                .map(|bytes| {
                    let request_id = HostRequestId::from_bytes(
                        bytes
                            .try_into()
                            .map_err(|_| StorageError::InvalidActivity)?,
                    );
                    request(connection, request_id).map(|value| value.map(|item| item.0))
                })
                .transpose()
                .map(Option::flatten)
        })
    }

    pub fn admit_waiting_download(
        &self,
        episode_id: EpisodeId,
        expected_revision: StateRevision,
        issued_revision: StateRevision,
        now_ms: i64,
        deadline_at_ms: i64,
    ) -> Result<DownloadWorkflowTransition, StorageError> {
        crate::transition_commit::commit_waiting_download_reconciliation(
            self.path(),
            episode_id,
            expected_revision,
            issued_revision,
            now_ms,
            Some(deadline_at_ms),
        )
    }

    pub fn retire_obsolete_waiting_download(
        &self,
        episode_id: EpisodeId,
        expected_revision: StateRevision,
        now_ms: i64,
    ) -> Result<DownloadWorkflowTransition, StorageError> {
        crate::transition_commit::commit_waiting_download_reconciliation(
            self.path(),
            episode_id,
            expected_revision,
            StateRevision::INITIAL,
            now_ms,
            None,
        )
    }
}

pub(crate) fn apply_waiting_download_admission(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: EpisodeId,
    expected_revision: StateRevision,
    issued_revision: StateRevision,
    now_ms: i64,
    deadline_at_ms: i64,
) -> Result<DownloadWorkflowTransition, StorageError> {
    let existing =
        workflow(transaction, episode_id)?.ok_or(StorageError::DownloadWorkflowNotFound)?;
    if existing.stage != StoredDownloadStage::Waiting
        || existing.workflow_revision != expected_revision
    {
        return Err(StorageError::DownloadWorkflowConflict);
    }
    let attempt = existing
        .attempt
        .checked_add(1)
        .ok_or(StorageError::DownloadWorkflowConflict)?;
    let attempt_id = download_attempt_identity(existing.intent_id, attempt)
        .ok_or(StorageError::DownloadWorkflowConflict)?;
    let request_id = download_start_request_id(attempt_id);
    transaction
        .execute(
            "UPDATE pod0_download_workflows SET stage='requested',\
                     workflow_revision=workflow_revision+1,attempt=?1,attempt_id=?2,request_id=?3,\
                     issued_revision=?4,deadline_at_ms=?5,not_before_ms=NULL,failure_code=NULL,\
                     failure_detail=NULL,failure_retryable=0,updated_at_ms=?6 WHERE episode_id=?7 \
                     AND workflow_revision=?8 AND stage='waiting'",
            params![
                i64::from(attempt),
                attempt_id.into_bytes().as_slice(),
                request_id.into_bytes().as_slice(),
                u64_to_i64(issued_revision.value)?,
                deadline_at_ms,
                now_ms,
                episode_id.into_bytes().as_slice(),
                u64_to_i64(expected_revision.value)?,
            ],
        )
        .map_err(|error| StorageError::sqlite("admit waiting download", error))?;
    if transaction.changes() != 1 {
        return Err(StorageError::DownloadWorkflowConflict);
    }
    insert_attempt_and_start_request(
        transaction,
        episode_id,
        existing.intent_id,
        attempt,
        attempt_id,
        request_id,
        existing.command_id,
        existing.cancellation_id,
        issued_revision,
        deadline_at_ms,
        &existing.input_version,
        &existing.enclosure_url,
        existing.resume_key.as_deref(),
        now_ms,
    )?;
    let record =
        workflow(transaction, episode_id)?.ok_or(StorageError::DownloadWorkflowNotFound)?;
    Ok(DownloadWorkflowTransition {
        record,
        replaced: Some(Box::new(existing)),
    })
}

pub(crate) fn apply_obsolete_waiting_download(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: EpisodeId,
    expected_revision: StateRevision,
    now_ms: i64,
) -> Result<DownloadWorkflowTransition, StorageError> {
    let existing =
        workflow(transaction, episode_id)?.ok_or(StorageError::DownloadWorkflowNotFound)?;
    if existing.stage != StoredDownloadStage::Waiting
        || existing.workflow_revision != expected_revision
    {
        return Err(StorageError::DownloadWorkflowConflict);
    }
    transaction
                .execute(
                    "UPDATE pod0_download_workflows SET desired_state='absent',stage='cancelled',\
                     workflow_revision=workflow_revision+1,request_id=NULL,deadline_at_ms=NULL,\
                     not_before_ms=NULL,failure_code=NULL,failure_detail=NULL,failure_retryable=0,\
                     updated_at_ms=?1 WHERE episode_id=?2 AND workflow_revision=?3 AND stage='waiting'",
                    params![
                        now_ms,
                        episode_id.into_bytes().as_slice(),
                        u64_to_i64(expected_revision.value)?
                    ],
                )
                .map_err(|error| StorageError::sqlite("retire obsolete download", error))?;
    if transaction.changes() != 1 {
        return Err(StorageError::DownloadWorkflowConflict);
    }
    let record =
        workflow(transaction, episode_id)?.ok_or(StorageError::DownloadWorkflowNotFound)?;
    Ok(DownloadWorkflowTransition {
        record,
        replaced: Some(Box::new(existing)),
    })
}
