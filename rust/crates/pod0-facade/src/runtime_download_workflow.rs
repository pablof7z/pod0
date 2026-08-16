use pod0_application::MAX_ACTIVE_DOWNLOAD_WORKFLOWS;
use pod0_domain::HostRequestId;
use pod0_storage::DownloadHostRequestKind;

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
            let outcome = store.reconcile_download_timeout(pod0_storage::DownloadFailureInput {
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

    pub(super) fn withdraw_download_request(&mut self, request_id: HostRequestId) {
        self.host_requests.cancel_request(request_id);
        self.host_requests.retire(request_id);
    }

    pub(super) fn retire_download_request(&mut self, request_id: HostRequestId) {
        self.host_requests.retire(request_id);
    }
}
