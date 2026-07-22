use pod0_application::{
    HostCancellationRequest, HostRequest, HostRequestEnvelope, MAX_SCHEDULED_AGENT_TASKS,
};
use pod0_domain::HostRequestId;
use pod0_storage::{ScheduledAgentHostRequestRecord, StorageError};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(crate) fn rehydrate_scheduled_agent_workflows(&mut self) -> Result<(), StorageError> {
        let Some(store) = self.scheduled_agent_store.clone() else {
            return Ok(());
        };
        let recovery = store.recover_after_restart(self.now())?;
        for request in recovery.reissued_requests {
            self.queue_scheduled_agent_request(request)?;
        }
        for occurrence_id in recovery.ambiguous_occurrences {
            if let Some(occurrence) = store.occurrence(occurrence_id)? {
                self.revision = pod0_domain::StateRevision::new(
                    self.revision.value.max(occurrence.revision.value),
                );
            }
        }
        Ok(())
    }

    pub(crate) fn admit_scheduled_agent_requests(&mut self) -> Result<(), StorageError> {
        let records = self
            .scheduled_agent_store
            .as_ref()
            .map(|store| store.pending_host_requests(MAX_SCHEDULED_AGENT_TASKS))
            .transpose()?
            .unwrap_or_default();
        for record in records {
            self.queue_scheduled_agent_request(record)?;
        }
        Ok(())
    }

    pub(super) fn queue_scheduled_agent_request(
        &mut self,
        record: ScheduledAgentHostRequestRecord,
    ) -> Result<bool, StorageError> {
        if self
            .pending_scheduled_agents
            .contains_key(&record.request_id)
        {
            return Ok(true);
        }
        if self.pending_scheduled_agents.len() >= usize::from(MAX_SCHEDULED_AGENT_TASKS) {
            return Ok(false);
        }
        let envelope = scheduled_agent_host_request(&record);
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
        self.revision =
            pod0_domain::StateRevision::new(self.revision.value.max(record.issued_revision.value));
        self.pending_scheduled_agents
            .insert(record.request_id, record);
        Ok(true)
    }

    pub(super) fn withdraw_scheduled_agent_request(&mut self, request_id: HostRequestId) {
        let was_queued = self
            .host_queue
            .iter()
            .any(|request| request.request_id == request_id);
        self.host_queue
            .retain(|request| request.request_id != request_id);
        let pending = self.pending_scheduled_agents.remove(&request_id);
        self.pending_scheduled_agent_observations
            .remove(&request_id);
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

    pub(super) fn retire_scheduled_agent_request(&mut self, request_id: HostRequestId) {
        self.pending_scheduled_agents.remove(&request_id);
        self.pending_scheduled_agent_observations
            .remove(&request_id);
        self.host_requests.retire(request_id);
        let _ = self.admit_scheduled_agent_requests();
    }
}

fn scheduled_agent_host_request(record: &ScheduledAgentHostRequestRecord) -> HostRequestEnvelope {
    HostRequestEnvelope {
        request_id: record.request_id,
        command_id: record.command_id,
        cancellation_id: record.cancellation_id,
        issued_revision: record.issued_revision,
        deadline_at: Some(record.deadline_at),
        request: HostRequest::ExecuteScheduledAgentTurn {
            execution: record.execution.clone(),
        },
    }
}
