use pod0_application::{AgentTurnState, AgentWorkflowAcceptance, HostObservationEnvelope};
use pod0_domain::{CommandId, StateRevision};
use pod0_storage::{AgentAuditKind, AgentStore, StorageError};

use crate::runtime_agent_modules::persistence::persist_agent_update;

pub(crate) fn fail_invalid_model_action(
    store: &AgentStore,
    mut state: AgentTurnState,
    envelope: &HostObservationEnvelope,
    expected_revision: StateRevision,
) -> Result<AgentTurnState, StorageError> {
    if state.fail_model(Some("invalid_tool_action".into()), envelope.observed_at)
        != AgentWorkflowAcceptance::Updated
    {
        return Err(StorageError::AgentTurnConflict);
    }
    persist_agent_update(
        store,
        CommandId::from_bytes(envelope.request_id.into_bytes()),
        b"pod0:agent-model-invalid-action:v1",
        AgentAuditKind::ModelObserved,
        expected_revision,
        state,
        envelope.observed_at,
    )
}
