use pod0_application::{AgentTurnStage, AgentTurnState};
use sha2::{Digest as _, Sha256};

use crate::StorageError;

pub(crate) const AGENT_STATE_SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_AGENT_STATE_BYTES: usize = 1_048_576;

pub(crate) fn encode_state(state: &AgentTurnState) -> Result<(Vec<u8>, [u8; 32]), StorageError> {
    if !state.is_valid_for_recovery() {
        return Err(StorageError::InvalidAgentState);
    }
    let bytes = serde_json::to_vec(state).map_err(|_| StorageError::InvalidAgentState)?;
    if !(2..=MAX_AGENT_STATE_BYTES).contains(&bytes.len()) {
        return Err(StorageError::InvalidAgentState);
    }
    Ok((bytes.clone(), Sha256::digest(&bytes).into()))
}

pub(crate) fn decode_state(
    bytes: &[u8],
    expected_digest: &[u8],
) -> Result<AgentTurnState, StorageError> {
    if !(2..=MAX_AGENT_STATE_BYTES).contains(&bytes.len())
        || expected_digest.len() != 32
        || Sha256::digest(bytes).as_slice() != expected_digest
    {
        return Err(StorageError::CorruptSchema {
            detail: "agent state digest is invalid",
        });
    }
    let state: AgentTurnState =
        serde_json::from_slice(bytes).map_err(|_| StorageError::CorruptSchema {
            detail: "agent state payload is invalid",
        })?;
    if !state.is_valid_for_recovery() {
        return Err(StorageError::CorruptSchema {
            detail: "agent state invariants are invalid",
        });
    }
    Ok(state)
}

pub(crate) const fn stage_code(stage: AgentTurnStage) -> &'static str {
    match stage {
        AgentTurnStage::AwaitingModel => "awaiting_model",
        AgentTurnStage::ApprovalRequired => "approval_required",
        AgentTurnStage::Authorized => "authorized",
        AgentTurnStage::Executing => "executing",
        AgentTurnStage::CommitPending => "commit_pending",
        AgentTurnStage::Committed => "committed",
        AgentTurnStage::Completed => "completed",
        AgentTurnStage::Denied => "denied",
        AgentTurnStage::Cancelled => "cancelled",
        AgentTurnStage::Blocked => "blocked",
        AgentTurnStage::OutcomeAmbiguous => "outcome_ambiguous",
        AgentTurnStage::Failed => "failed",
    }
}
