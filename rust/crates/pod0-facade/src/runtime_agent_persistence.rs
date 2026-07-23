use pod0_application::AgentTurnState;
use pod0_domain::{CommandId, StateRevision, UnixTimestampMilliseconds};
use pod0_storage::{
    AgentAuditKind, AgentCommandContext, AgentMutationOutcome, AgentTurnMutation, StorageError,
};

use crate::runtime_agent_modules::identity::agent_fingerprint;

pub(super) fn persist_agent_update(
    store: &pod0_storage::AgentStore,
    command_id: CommandId,
    fingerprint_domain: &[u8],
    audit_kind: AgentAuditKind,
    expected_revision: StateRevision,
    state: AgentTurnState,
    observed_at: UnixTimestampMilliseconds,
) -> Result<AgentTurnState, StorageError> {
    let fingerprint = agent_fingerprint(
        fingerprint_domain,
        &[
            &state.projection().turn_id.into_bytes(),
            &expected_revision.value.to_be_bytes(),
        ],
    );
    let outcome = store.update_turn(
        AgentCommandContext {
            command_id,
            command_fingerprint: fingerprint,
            observed_at,
        },
        AgentTurnMutation {
            expected_revision,
            audit_kind,
        },
        &state,
    )?;
    Ok(match outcome {
        AgentMutationOutcome::Applied(state) | AgentMutationOutcome::Duplicate(state) => state,
    })
}
