use crate::{
    AgentTurnProjection, AgentTurnStage, AgentTurnState, AgentWorkflowAcceptance,
    MAX_AGENT_MESSAGE_BYTES, MAX_AGENT_PROJECTION_MESSAGES, MAX_AGENT_RECALL_EVIDENCE,
    MAX_AGENT_TOOLS_PER_TURN, agent_proposal_identity, agent_tool_policy, validate_agent_action,
    validate_agent_model_reference,
};
use pod0_domain::{StateRevision, UnixTimestampMilliseconds};

impl AgentTurnProjection {
    pub fn enforce_bounds(&mut self, requested_items: usize) {
        let limit = requested_items.clamp(1, MAX_AGENT_PROJECTION_MESSAGES);
        if self.messages.len() > limit {
            self.messages.drain(..self.messages.len() - limit);
        }
        self.recall_evidence
            .truncate(usize::from(MAX_AGENT_RECALL_EVIDENCE));
    }
}

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

    pub fn mark_outcome_ambiguous(
        &mut self,
        observed_at: UnixTimestampMilliseconds,
    ) -> AgentWorkflowAcceptance {
        if !matches!(
            self.projection.stage,
            AgentTurnStage::AwaitingModel | AgentTurnStage::Executing
        ) {
            return AgentWorkflowAcceptance::Rejected;
        }
        self.advance(AgentTurnStage::OutcomeAmbiguous, observed_at);
        AgentWorkflowAcceptance::Updated
    }

    pub(super) fn advance(
        &mut self,
        stage: AgentTurnStage,
        observed_at: UnixTimestampMilliseconds,
    ) {
        self.projection.revision = StateRevision::new(self.projection.revision.value + 1);
        self.projection.stage = stage;
        self.projection.updated_at = observed_at;
    }

    pub(super) fn fail(&mut self, code: &str, observed_at: UnixTimestampMilliseconds) {
        self.projection.safe_failure = Some(code.to_owned());
        self.projection.execution_fence_id = None;
        self.advance(AgentTurnStage::Failed, observed_at);
    }
}
