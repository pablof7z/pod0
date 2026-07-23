use crate::{
    AgentTurnProjection, AgentTurnState, MAX_AGENT_MESSAGE_BYTES, MAX_AGENT_TOOLS_PER_TURN,
    agent_proposal_identity, agent_tool_policy, validate_agent_action,
    validate_agent_model_reference,
};

impl AgentTurnState {
    #[must_use]
    pub fn projection(&self) -> AgentTurnProjection {
        self.projection.clone()
    }

    #[must_use]
    pub fn model_reference(&self) -> &str {
        &self.model_reference
    }

    #[must_use]
    pub fn available_tools(&self) -> &[crate::AgentToolName] {
        &self.available_tools
    }

    #[must_use]
    pub const fn cancellation_id(&self) -> pod0_domain::CancellationId {
        self.cancellation_id
    }

    #[must_use]
    pub fn is_valid_for_recovery(&self) -> bool {
        validate_agent_model_reference(&self.model_reference).is_ok()
            && !self.projection.messages.is_empty()
            && !self.available_tools.is_empty()
            && self.available_tools.len() <= MAX_AGENT_TOOLS_PER_TURN
            && self
                .available_tools
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.available_tools.len()
            && self.projection.messages.iter().all(|message| {
                !message.content.is_empty() && message.content.len() <= MAX_AGENT_MESSAGE_BYTES
            })
            && self.projection.proposal.as_ref().is_none_or(|proposal| {
                validate_agent_action(&proposal.action).is_ok()
                    && agent_proposal_identity(
                        self.projection.turn_id,
                        proposal.revision,
                        &proposal.action,
                    ) == (proposal.proposal_id, proposal.proposal_digest)
                    && agent_tool_policy(proposal.action.tool()).authority
                        == proposal.required_authority
            })
    }
}
