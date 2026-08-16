use pod0_application::{ActivitySubject, DurableEffectExecution, HostRequest, HostRequestEnvelope};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn agent_capability_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let DurableEffectExecution::AgentCapability { request } = &lease.request.execution else {
            return None;
        };
        exact_agent_subject(lease, request.capability.turn_id, request.deadline_at)?;
        Some(HostRequestEnvelope {
            request_id: request.request_id,
            command_id: request.command_id,
            cancellation_id: request.cancellation_id,
            issued_revision: request.issued_revision,
            deadline_at: request.deadline_at,
            request: HostRequest::ExecuteAgentCapability {
                capability: request.capability.clone(),
            },
        })
    }

    pub(super) fn agent_approval_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let DurableEffectExecution::AgentApproval { request } = &lease.request.execution else {
            return None;
        };
        exact_agent_subject(lease, request.approval.turn_id, request.deadline_at)?;
        Some(HostRequestEnvelope {
            request_id: request.request_id,
            command_id: request.command_id,
            cancellation_id: request.cancellation_id,
            issued_revision: request.issued_revision,
            deadline_at: request.deadline_at,
            request: HostRequest::PresentAgentApproval {
                approval: request.approval.clone(),
            },
        })
    }

    pub(super) fn agent_model_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let DurableEffectExecution::AgentModel { request } = &lease.request.execution else {
            return None;
        };
        exact_agent_subject(lease, request.execution.turn_id, request.deadline_at)?;
        Some(HostRequestEnvelope {
            request_id: request.request_id,
            command_id: request.command_id,
            cancellation_id: request.cancellation_id,
            issued_revision: request.issued_revision,
            deadline_at: request.deadline_at,
            request: HostRequest::ExecuteAgentModelTurn {
                execution: request.execution.clone(),
            },
        })
    }
}

fn exact_agent_subject(
    lease: &pod0_storage::EffectLease,
    turn_id: pod0_domain::AgentTurnId,
    deadline_at: Option<pod0_domain::UnixTimestampMilliseconds>,
) -> Option<()> {
    (lease.subject == (ActivitySubject::AgentTurn { turn_id })
        && lease.request.subject == lease.subject
        && lease.request.deadline_at == deadline_at)
        .then_some(())
}
