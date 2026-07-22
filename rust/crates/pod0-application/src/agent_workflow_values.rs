use pod0_domain::{
    AgentAuthorizationId, AgentExecutionFenceId, AgentProposalId, AgentTurnId, ContentDigest,
    ConversationId, GeneratedArtifactId, UnixTimestampMilliseconds,
};

use crate::{AgentAuthority, AgentToolAction, AgentTurnProjection};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentTurnState {
    pub(super) projection: AgentTurnProjection,
    pub(super) model_fence_id: AgentExecutionFenceId,
    pub(super) authorization_id: Option<AgentAuthorizationId>,
    pub(super) action_observation: Option<AgentActionObservation>,
    pub(super) model_reference: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentTurnStart {
    pub conversation_id: ConversationId,
    pub turn_id: AgentTurnId,
    pub model_fence_id: AgentExecutionFenceId,
    pub user_input: String,
    pub model_reference: String,
    pub observed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentModelObservation {
    pub turn_id: AgentTurnId,
    pub model_fence_id: AgentExecutionFenceId,
    pub assistant_text: String,
    pub proposed_action: Option<AgentToolAction>,
    pub observed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentAuthorizationObservation {
    pub proposal_id: AgentProposalId,
    pub proposal_digest: ContentDigest,
    pub authority: AgentAuthority,
    pub authorization_id: AgentAuthorizationId,
    pub approved: bool,
    pub observed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentActionObservation {
    pub proposal_id: AgentProposalId,
    pub execution_fence_id: AgentExecutionFenceId,
    pub outcome: AgentActionOutcome,
    pub observed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AgentActionOutcome {
    Succeeded {
        bounded_result: String,
        artifact_id: Option<GeneratedArtifactId>,
    },
    Failed {
        safe_detail: Option<String>,
    },
    Cancelled,
    OutcomeAmbiguous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentWorkflowAcceptance {
    Updated,
    Duplicate,
    Stale,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentTurnStartError {
    InvalidInput,
    InvalidModelReference,
}
