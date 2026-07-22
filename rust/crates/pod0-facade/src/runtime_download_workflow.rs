use pod0_application::{
    HostCancellationRequest, HostRequest, HostRequestEnvelope, MAX_ACTIVE_DOWNLOAD_WORKFLOWS,
};
use pod0_domain::{HostRequestId, UnixTimestampMilliseconds};
use pod0_storage::{DownloadHostRequestKind, DownloadHostRequestRecord, StoredDownloadStage};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn reconcile_download_deadlines(&mut self) -> bool {
        let Some(store) = self.store.clone() else {
            return false;
        };
        let now = self.now();
        let requests = match store.pending_download_host_requests(MAX_ACTIVE_DOWNLOAD_WORKFLOWS) {
            Ok(value) => value,
            Err(_) => return false,
        };
        let mut changed = false;
        for request in requests.into_iter().filter(|request| {
            request
                .deadline_at_ms
                .is_some_and(|deadline| deadline <= now.value)
        }) {
            let sequence = request.last_sequence_number.unwrap_or(0).saturating_add(1);
            let retry_at = pod0_application::download_retry_not_before(now).value;
            let retry_deadline =
                retry_at.checked_add(pod0_application::DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS);
            let outcome = store.fail_download_host_request(pod0_storage::DownloadFailureInput {
                request_id: request.request_id,
                sequence_number: sequence,
                failure_code: "timed_out".to_owned(),
                failure_detail: None,
                retryable: request.kind == DownloadHostRequestKind::Start,
                retry_at_ms: (request.kind == DownloadHostRequestKind::Start).then_some(retry_at),
                retry_deadline_at_ms: retry_deadline,
                issued_revision: self.revision,
                observed_at_ms: now.value,
            });
            if let Ok(pod0_storage::DownloadObservationOutcome::Updated(record)) = outcome {
                self.withdraw_download_request(request.request_id);
                self.revision = pod0_domain::StateRevision::new(
                    self.revision.value.max(record.workflow_revision.value),
                );
                self.finish_download_operation(&request, &record);
                changed = true;
            }
        }
        changed
    }

    pub(super) fn rehydrate_download_workflows(
        &mut self,
    ) -> Result<(), pod0_storage::StorageError> {
        let records = self
            .store
            .as_ref()
            .map(|store| store.pending_download_host_requests(MAX_ACTIVE_DOWNLOAD_WORKFLOWS))
            .transpose()?
            .unwrap_or_default();
        for record in records {
            self.queue_download_request(record)?;
        }
        let workflows = self
            .store
            .as_ref()
            .map(|store| store.download_workflow_page(None, 0, MAX_ACTIVE_DOWNLOAD_WORKFLOWS))
            .transpose()?
            .map(|page| page.items)
            .unwrap_or_default();
        for workflow in workflows {
            self.revision = pod0_domain::StateRevision::new(
                self.revision.value.max(workflow.workflow_revision.value),
            );
        }
        Ok(())
    }

    pub(super) fn admit_download_requests(&mut self) -> Result<(), pod0_storage::StorageError> {
        let records = self
            .store
            .as_ref()
            .map(|store| store.pending_download_host_requests(MAX_ACTIVE_DOWNLOAD_WORKFLOWS))
            .transpose()?
            .unwrap_or_default();
        for record in records {
            self.queue_download_request(record)?;
        }
        Ok(())
    }

    pub(super) fn queue_download_request(
        &mut self,
        record: DownloadHostRequestRecord,
    ) -> Result<bool, pod0_storage::StorageError> {
        if self.pending_downloads.contains_key(&record.request_id) {
            return Ok(true);
        }
        if self.pending_downloads.len() >= usize::from(MAX_ACTIVE_DOWNLOAD_WORKFLOWS) {
            return Ok(false);
        }
        if record.kind == DownloadHostRequestKind::Start {
            let workflow = self
                .store
                .as_ref()
                .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)?
                .download_workflow(record.episode_id)?
                .ok_or(pod0_storage::StorageError::DownloadWorkflowNotFound)?;
            if workflow.stage == StoredDownloadStage::RetryScheduled
                && workflow
                    .not_before_ms
                    .is_some_and(|not_before| not_before > self.now().value)
            {
                return Ok(false);
            }
        }
        let envelope = host_request(&record)?;
        if !self.host_requests.register(envelope.clone())
            && !self.host_requests.matches_outstanding(&envelope)
        {
            return Ok(false);
        }
        if !self
            .host_queue
            .iter()
            .any(|queued| queued.request_id == record.request_id)
        {
            self.host_queue.push_back(envelope);
        }
        self.pending_downloads.insert(record.request_id, record);
        Ok(true)
    }

    pub(super) fn withdraw_download_request(&mut self, request_id: HostRequestId) {
        let was_queued = self
            .host_queue
            .iter()
            .any(|request| request.request_id == request_id);
        self.host_queue
            .retain(|request| request.request_id != request_id);
        let pending = self.pending_downloads.remove(&request_id);
        self.pending_download_observations.remove(&request_id);
        if self.host_requests.cancel_request(request_id)
            && !was_queued
            && let Some(record) = pending
        {
            self.host_cancellations.push_back(HostCancellationRequest {
                request_id,
                cancellation_id: record.cancellation_id,
            });
        }
        self.host_requests.retire(request_id);
    }

    pub(super) fn retire_download_request(&mut self, request_id: HostRequestId) {
        self.pending_downloads.remove(&request_id);
        self.pending_download_observations.remove(&request_id);
        self.host_requests.retire(request_id);
        let _ = self.admit_download_requests();
    }
}

fn host_request(
    record: &DownloadHostRequestRecord,
) -> Result<HostRequestEnvelope, pod0_storage::StorageError> {
    let request = match record.kind {
        DownloadHostRequestKind::Start => HostRequest::StartEpisodeDownload {
            episode_id: record.episode_id,
            intent_id: record
                .intent_id
                .ok_or(pod0_storage::StorageError::DownloadWorkflowConflict)?,
            attempt_id: record
                .attempt_id
                .ok_or(pod0_storage::StorageError::DownloadWorkflowConflict)?,
            input_version: record
                .input_version
                .clone()
                .ok_or(pod0_storage::StorageError::DownloadWorkflowConflict)?,
            enclosure_url: record
                .enclosure_url
                .clone()
                .ok_or(pod0_storage::StorageError::DownloadWorkflowConflict)?,
            resume_key: record.resume_key.clone(),
        },
        DownloadHostRequestKind::Cancel => HostRequest::CancelEpisodeDownload {
            episode_id: record.episode_id,
            intent_id: record
                .intent_id
                .ok_or(pod0_storage::StorageError::DownloadWorkflowConflict)?,
            attempt_id: record
                .attempt_id
                .ok_or(pod0_storage::StorageError::DownloadWorkflowConflict)?,
            external_task_key: record.external_task_key.clone(),
        },
        DownloadHostRequestKind::Remove => HostRequest::RemoveEpisodeDownloadArtifact {
            episode_id: record.episode_id,
            artifact_key: record
                .artifact_key
                .clone()
                .ok_or(pod0_storage::StorageError::DownloadWorkflowConflict)?,
        },
    };
    Ok(HostRequestEnvelope {
        request_id: record.request_id,
        command_id: record.command_id,
        cancellation_id: record.cancellation_id,
        issued_revision: record.issued_revision,
        deadline_at: record.deadline_at_ms.map(UnixTimestampMilliseconds::new),
        request,
    })
}
