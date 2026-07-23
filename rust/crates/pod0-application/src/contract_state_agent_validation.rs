use crate::{
    AgentCapabilityOutcome, HostObservation, HostRequest, MAX_AGENT_MESSAGE_BYTES,
    MAX_AGENT_SAFE_DETAIL_BYTES, MAX_AGENT_TOOL_ARGUMENTS_BYTES, MAX_AGENT_TOOL_CALL_ID_BYTES,
    MAX_AGENT_TOOL_NAME_BYTES,
};

pub(super) fn agent_observation_matches(
    request: &HostRequest,
    observation: &HostObservation,
) -> Option<bool> {
    let matches = match (request, observation) {
        (
            HostRequest::ExecuteAgentModelTurn { execution },
            HostObservation::AgentModelCompleted {
                turn_id,
                model_fence_id,
                assistant_text,
                proposed_tool_call,
                usage,
            },
        ) => {
            *turn_id == execution.turn_id
                && *model_fence_id == execution.model_fence_id
                && assistant_text.len() <= execution.maximum_output_bytes as usize
                && assistant_text.len() <= MAX_AGENT_MESSAGE_BYTES
                && proposed_tool_call.as_ref().is_none_or(tool_call_is_bounded)
                && usage.is_none_or(crate::agent_model_usage_is_valid)
        }
        (
            HostRequest::PresentAgentApproval { approval },
            HostObservation::AgentApprovalObserved {
                turn_id,
                proposal_id,
                proposal_digest,
                ..
            },
        ) => {
            *turn_id == approval.turn_id
                && *proposal_id == approval.proposal.proposal_id
                && *proposal_digest == approval.proposal.proposal_digest
        }
        (
            HostRequest::ExecuteAgentCapability { capability },
            HostObservation::AgentCapabilityObserved {
                turn_id,
                proposal_id,
                execution_fence_id,
                outcome,
            },
        ) => {
            *turn_id == capability.turn_id
                && *proposal_id == capability.proposal_id
                && *execution_fence_id == capability.execution_fence_id
                && outcome_is_bounded(capability, outcome)
        }
        (
            HostRequest::ExecuteAgentModelTurn { .. }
            | HostRequest::PresentAgentApproval { .. }
            | HostRequest::ExecuteAgentCapability { .. },
            _,
        ) => false,
        _ => return None,
    };
    Some(matches)
}

fn tool_call_is_bounded(call: &crate::AgentModelToolCallObservation) -> bool {
    !call.provider_call_id.is_empty()
        && call.provider_call_id.len() <= MAX_AGENT_TOOL_CALL_ID_BYTES
        && !call.tool_name.is_empty()
        && call.tool_name.len() <= MAX_AGENT_TOOL_NAME_BYTES
        && call.arguments_json.len() <= MAX_AGENT_TOOL_ARGUMENTS_BYTES
}

fn outcome_is_bounded(
    capability: &crate::AgentCapabilityRequest,
    outcome: &AgentCapabilityOutcome,
) -> bool {
    match outcome {
        AgentCapabilityOutcome::Succeeded { bounded_result } => {
            capability.generated_audio_target.is_none()
                && bounded_result.len() <= MAX_AGENT_MESSAGE_BYTES
        }
        AgentCapabilityOutcome::GeneratedAudioStaged { evidence } => capability
            .generated_audio_target
            .is_some_and(|target| crate::agent_generated_audio_evidence_is_valid(evidence, target)),
        AgentCapabilityOutcome::Failed { safe_detail } => safe_detail
            .as_ref()
            .is_none_or(|value| value.len() <= MAX_AGENT_SAFE_DETAIL_BYTES),
        AgentCapabilityOutcome::Cancelled | AgentCapabilityOutcome::OutcomeAmbiguous => true,
    }
}
