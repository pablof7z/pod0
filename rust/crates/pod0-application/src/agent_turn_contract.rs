use crate::{
    AgentAuthority, AgentCapabilityExecutionMode, AgentGeneratedAudioEvidence,
    AgentGeneratedAudioTarget, AgentMessageProjection, AgentToolAction, RecallEvidenceProjection,
};
use pod0_domain::{
    AgentCommitId, AgentExecutionFenceId, AgentProposalId, AgentTurnId, ContentDigest,
    ConversationId, GeneratedArtifactId, StateRevision, UnixTimestampMilliseconds,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum AgentTurnStage {
    AwaitingModel,
    ApprovalRequired,
    Authorized,
    Executing,
    CommitPending,
    Committed,
    Completed,
    Denied,
    Cancelled,
    Blocked,
    OutcomeAmbiguous,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct AgentProposalProjection {
    pub proposal_id: AgentProposalId,
    pub proposal_digest: ContentDigest,
    pub revision: StateRevision,
    pub action: AgentToolAction,
    pub required_authority: AgentAuthority,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct AgentCommitReceipt {
    pub commit_id: AgentCommitId,
    pub proposal_id: AgentProposalId,
    pub artifact_id: Option<GeneratedArtifactId>,
    pub committed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct AgentTurnProjection {
    pub conversation_id: ConversationId,
    pub turn_id: AgentTurnId,
    pub revision: StateRevision,
    pub stage: AgentTurnStage,
    pub messages: Vec<AgentMessageProjection>,
    #[serde(default)]
    pub recall_evidence: Vec<RecallEvidenceProjection>,
    #[serde(default)]
    pub model_usage: Vec<crate::AgentModelUsageProjection>,
    pub proposal: Option<AgentProposalProjection>,
    pub execution_fence_id: Option<AgentExecutionFenceId>,
    pub commit: Option<AgentCommitReceipt>,
    pub safe_failure: Option<String>,
    pub updated_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentConversationProjection {
    pub conversation_id: ConversationId,
    pub turns: Vec<AgentTurnProjection>,
    pub has_more: bool,
    pub failure: Option<crate::CoreFailure>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentModelExecutionRequest {
    pub conversation_id: ConversationId,
    pub turn_id: AgentTurnId,
    pub model_fence_id: AgentExecutionFenceId,
    pub model_reference: String,
    pub messages: Vec<AgentMessageProjection>,
    pub tool_definitions: Vec<crate::AgentToolDefinition>,
    pub maximum_output_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentApprovalRequest {
    pub turn_id: AgentTurnId,
    pub proposal: AgentProposalProjection,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentCapabilityRequest {
    pub turn_id: AgentTurnId,
    pub proposal_id: AgentProposalId,
    pub proposal_digest: ContentDigest,
    pub execution_fence_id: AgentExecutionFenceId,
    pub execution_mode: AgentCapabilityExecutionMode,
    pub generated_audio_target: Option<AgentGeneratedAudioTarget>,
    pub action: AgentToolAction,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AgentCapabilityOutcome {
    Succeeded {
        bounded_result: String,
    },
    GeneratedAudioStaged {
        evidence: AgentGeneratedAudioEvidence,
    },
    Failed {
        safe_detail: Option<String>,
    },
    Cancelled,
    OutcomeAmbiguous,
}
