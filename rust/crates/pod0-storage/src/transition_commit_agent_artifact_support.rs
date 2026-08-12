use pod0_application::AgentToolAction;
use pod0_domain::{AgentProposalId, AgentTurnId};

use crate::StorageError;

pub(super) fn require_current_proposal(
    transaction: &rusqlite::Transaction<'_>,
    turn_id: AgentTurnId,
    proposal_id: AgentProposalId,
    action: &AgentToolAction,
) -> Result<(), StorageError> {
    let state = crate::agent_store::read_turn(transaction, turn_id)?
        .ok_or(StorageError::AgentTurnNotFound)?;
    let proposal = state
        .projection()
        .proposal
        .ok_or(StorageError::InvalidAgentState)?;
    if proposal.proposal_id == proposal_id && proposal.action == *action {
        Ok(())
    } else {
        Err(StorageError::AgentTurnConflict)
    }
}

pub(super) fn hex(value: [u8; 32]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}
