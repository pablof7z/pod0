use pod0_application::{HostObservationEnvelope, HostRequestEnvelope};
use pod0_domain::{AgentTurnId, RecallQueryId};

#[derive(Clone, Debug)]
pub(crate) struct PendingAgentRequest {
    pub(super) turn_id: AgentTurnId,
    pub(super) envelope: HostRequestEnvelope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingAgentRecallObservation {
    pub(crate) query_id: RecallQueryId,
    pub(crate) envelope: HostObservationEnvelope,
}
