use pod0_application::HostRequestEnvelope;
use pod0_domain::AgentTurnId;

#[derive(Clone, Debug)]
pub(crate) struct PendingAgentRequest {
    pub(super) turn_id: AgentTurnId,
    pub(super) envelope: HostRequestEnvelope,
}
