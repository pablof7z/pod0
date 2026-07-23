use pod0_application::{
    AgentApprovalRequest, AgentCapabilityRequest, AgentMessageProjection,
    AgentModelExecutionRequest, AgentTurnState, HostCancellationRequest, HostRequest,
    HostRequestEnvelope, MAX_AGENT_MODEL_OUTPUT_BYTES, MAX_AGENT_PROJECTION_MESSAGES,
};
use pod0_domain::{AgentTurnId, CommandId, HostRequestId};

use crate::runtime_agent_modules::identity::{
    approval_request_id, capability_request_id, model_request_id,
};
use crate::runtime_agent_modules::state::PendingAgentRequest;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn queue_agent_model_request(
        &mut self,
        command_id: CommandId,
        state: &AgentTurnState,
    ) -> bool {
        let projection = state.projection();
        let Some(model_fence_id) = projection.execution_fence_id else {
            return false;
        };
        let messages = self.agent_model_messages(&projection);
        self.queue_agent_request(HostRequestEnvelope {
            request_id: model_request_id(projection.turn_id, model_fence_id),
            command_id,
            cancellation_id: state.cancellation_id(),
            issued_revision: self.revision,
            deadline_at: None,
            request: HostRequest::ExecuteAgentModelTurn {
                execution: AgentModelExecutionRequest {
                    conversation_id: projection.conversation_id,
                    turn_id: projection.turn_id,
                    model_fence_id,
                    model_reference: state.model_reference().to_owned(),
                    messages,
                    available_tools: state.available_tools().to_vec(),
                    maximum_output_bytes: MAX_AGENT_MODEL_OUTPUT_BYTES,
                },
            },
        })
    }

    fn agent_model_messages(
        &self,
        current: &pod0_application::AgentTurnProjection,
    ) -> Vec<AgentMessageProjection> {
        let Some(store) = &self.agent_store else {
            return current.messages.clone();
        };
        let Ok(mut page) = store.turn_page(
            current.conversation_id,
            0,
            MAX_AGENT_PROJECTION_MESSAGES as u16,
        ) else {
            return current.messages.clone();
        };
        page.items.reverse();
        let mut messages = page
            .items
            .into_iter()
            .flat_map(|turn| turn.messages)
            .collect::<Vec<_>>();
        if messages.len() > MAX_AGENT_PROJECTION_MESSAGES {
            messages.drain(..messages.len() - MAX_AGENT_PROJECTION_MESSAGES);
        }
        messages
    }

    pub(super) fn queue_agent_approval_request(
        &mut self,
        command_id: CommandId,
        state: &AgentTurnState,
    ) -> bool {
        let projection = state.projection();
        let Some(proposal) = projection.proposal else {
            return false;
        };
        self.queue_agent_request(HostRequestEnvelope {
            request_id: approval_request_id(
                projection.turn_id,
                proposal.proposal_id,
                proposal.proposal_digest,
            ),
            command_id,
            cancellation_id: state.cancellation_id(),
            issued_revision: self.revision,
            deadline_at: None,
            request: HostRequest::PresentAgentApproval {
                approval: AgentApprovalRequest {
                    turn_id: projection.turn_id,
                    proposal,
                },
            },
        })
    }

    pub(super) fn queue_agent_capability_request(
        &mut self,
        command_id: CommandId,
        state: &AgentTurnState,
    ) -> bool {
        let projection = state.projection();
        let (Some(proposal), Some(execution_fence_id)) =
            (projection.proposal, projection.execution_fence_id)
        else {
            return false;
        };
        self.queue_agent_request(HostRequestEnvelope {
            request_id: capability_request_id(
                projection.turn_id,
                proposal.proposal_id,
                execution_fence_id,
            ),
            command_id,
            cancellation_id: state.cancellation_id(),
            issued_revision: self.revision,
            deadline_at: None,
            request: HostRequest::ExecuteAgentCapability {
                capability: AgentCapabilityRequest {
                    turn_id: projection.turn_id,
                    proposal_id: proposal.proposal_id,
                    proposal_digest: proposal.proposal_digest,
                    execution_fence_id,
                    action: proposal.action,
                },
            },
        })
    }

    pub(super) fn withdraw_agent_requests(&mut self, turn_id: AgentTurnId) {
        let request_ids = self
            .pending_agents
            .iter()
            .filter_map(|(request_id, pending)| (pending.turn_id == turn_id).then_some(*request_id))
            .collect::<Vec<_>>();
        for request_id in request_ids {
            let was_queued = self
                .host_queue
                .iter()
                .any(|request| request.request_id == request_id);
            self.host_queue
                .retain(|request| request.request_id != request_id);
            let pending = self.pending_agents.remove(&request_id);
            if self.host_requests.cancel_request(request_id)
                && !was_queued
                && let Some(pending) = pending
            {
                self.host_cancellations.push_back(HostCancellationRequest {
                    request_id,
                    cancellation_id: pending.envelope.cancellation_id,
                });
            }
            self.host_requests.retire(request_id);
        }
    }

    pub(super) fn retire_agent_request(&mut self, request_id: HostRequestId) {
        self.pending_agents.remove(&request_id);
        self.host_requests.retire(request_id);
    }

    fn queue_agent_request(&mut self, envelope: HostRequestEnvelope) -> bool {
        if self.pending_agents.contains_key(&envelope.request_id) {
            return true;
        }
        if !self.host_requests.register(envelope.clone())
            && !self.host_requests.matches_outstanding(&envelope)
        {
            return false;
        }
        if !self
            .host_queue
            .iter()
            .any(|request| request.request_id == envelope.request_id)
        {
            self.host_queue.push_back(envelope.clone());
        }
        self.pending_agents.insert(
            envelope.request_id,
            PendingAgentRequest {
                turn_id: agent_turn_id(&envelope),
                envelope,
            },
        );
        true
    }
}

fn agent_turn_id(envelope: &HostRequestEnvelope) -> AgentTurnId {
    match &envelope.request {
        HostRequest::ExecuteAgentModelTurn { execution } => execution.turn_id,
        HostRequest::PresentAgentApproval { approval } => approval.turn_id,
        HostRequest::ExecuteAgentCapability { capability } => capability.turn_id,
        _ => unreachable!("agent queue received another host request"),
    }
}
