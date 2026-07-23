use pod0_domain::{AgentExecutionFenceId, UnixTimestampMilliseconds};

use crate::{AgentTurnStage, AgentTurnState, AgentWorkflowAcceptance};

impl AgentTurnState {
    /// Begins one bounded provider continuation after a successful tool action.
    ///
    /// The committed action receipt and tool evidence remain durable while the
    /// model produces the user-facing answer. The continuation may not propose
    /// another action, which keeps one exact authorization and commit per turn.
    pub fn continue_after_commit(
        &mut self,
        model_fence_id: AgentExecutionFenceId,
        observed_at: UnixTimestampMilliseconds,
    ) -> AgentWorkflowAcceptance {
        if self.projection.stage != AgentTurnStage::Committed
            || self.projection.commit.is_none()
            || self.action_observation.is_none()
        {
            return AgentWorkflowAcceptance::Rejected;
        }
        self.model_fence_id = model_fence_id;
        self.projection.execution_fence_id = Some(model_fence_id);
        self.advance(AgentTurnStage::AwaitingModel, observed_at);
        AgentWorkflowAcceptance::Updated
    }
}
