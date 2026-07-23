use pod0_application::{
    AgentActionOutcome, AgentCapabilityOutcome, AgentTurnStage, HostObservationReceipt,
    HostObservationRejection,
};
use pod0_domain::HostRequestId;

pub(super) fn map_capability_outcome(
    outcome: AgentCapabilityOutcome,
) -> Option<AgentActionOutcome> {
    Some(match outcome {
        AgentCapabilityOutcome::Succeeded { bounded_result } => AgentActionOutcome::Succeeded {
            bounded_result,
            artifact_id: None,
            recall_evidence: Vec::new(),
        },
        AgentCapabilityOutcome::GeneratedAudioStaged { .. } => return None,
        AgentCapabilityOutcome::Failed { safe_detail } => {
            AgentActionOutcome::Failed { safe_detail }
        }
        AgentCapabilityOutcome::Cancelled => AgentActionOutcome::Cancelled,
        AgentCapabilityOutcome::OutcomeAmbiguous => AgentActionOutcome::OutcomeAmbiguous,
    })
}

pub(super) fn is_terminal(stage: AgentTurnStage) -> bool {
    matches!(
        stage,
        AgentTurnStage::Committed
            | AgentTurnStage::Completed
            | AgentTurnStage::Denied
            | AgentTurnStage::Cancelled
            | AgentTurnStage::Blocked
            | AgentTurnStage::OutcomeAmbiguous
            | AgentTurnStage::Failed
    )
}

pub(super) fn retain(request_id: HostRequestId) -> HostObservationReceipt {
    HostObservationReceipt::RetainAndRetry { request_id }
}

pub(super) fn rejected(
    request_id: HostRequestId,
    reason: HostObservationRejection,
) -> HostObservationReceipt {
    HostObservationReceipt::Rejected { request_id, reason }
}
