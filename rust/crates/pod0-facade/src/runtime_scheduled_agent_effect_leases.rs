use pod0_application::{ActivitySubject, DurableEffectExecution, HostRequest, HostRequestEnvelope};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(crate) fn scheduled_agent_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let DurableEffectExecution::ScheduledAgent { request } = &lease.request.execution else {
            return None;
        };
        let subject = ActivitySubject::ScheduledOccurrence {
            occurrence_id: request.execution.occurrence_id,
        };
        if lease.subject != subject
            || lease.request.subject != subject
            || lease.request.deadline_at != Some(request.deadline_at)
        {
            return None;
        }
        Some(HostRequestEnvelope {
            request_id: request.request_id,
            command_id: request.command_id,
            cancellation_id: request.cancellation_id,
            issued_revision: request.issued_revision,
            deadline_at: Some(request.deadline_at),
            request: HostRequest::ExecuteScheduledAgentTurn {
                execution: request.execution.clone(),
            },
        })
    }
}
