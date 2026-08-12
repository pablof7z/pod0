use pod0_application::{
    AgentTurnStartActivityInput, AgentTurnStartMutation, AgentTurnState, plan_agent_turn_start,
};
use pod0_domain::{ContentDigest, StateRevision};

use super::TransitionCommit;
use crate::agent_store::{command_receipt, persist, read_turn};
use crate::{
    AgentAuditKind, AgentCommandContext, AgentMutationOutcome, AgentStore, StorageError,
    TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_agent_turn_start(
    path: &std::path::Path,
    context: AgentCommandContext,
    state: &AgentTurnState,
) -> Result<AgentMutationOutcome, StorageError> {
    let projection = state.projection();
    let store = AgentStore::open(path)?;
    let legacy_replay = std::cell::Cell::new(false);
    let ingress = TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: context.command_id.into_bytes(),
            fingerprint: ContentDigest::from_bytes(context.command_fingerprint),
        };
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        context.observed_at,
        |transaction| {
            let legacy = command_receipt(transaction, context, projection.turn_id)?;
            let current = legacy
                .as_ref()
                .map_or(StateRevision::INITIAL, |value| value.projection().revision);
            legacy_replay.set(legacy.is_some());
            plan_agent_turn_start(AgentTurnStartActivityInput {
                command_id: context.command_id,
                turn_id: projection.turn_id,
                current_revision: current,
                committed_revision: projection.revision,
                legacy_replay: legacy.is_some(),
            })
            .map(|plan| {
                plan.map_mutation(|mutation| {
                    (mutation, legacy.map(|value| value.projection().revision))
                })
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, _, (mutation, legacy_revision)| match mutation {
            AgentTurnStartMutation::Start => {
                let outcome = persist(transaction, context, None, AgentAuditKind::Started, state)?;
                Ok(outcome.state().projection().revision)
            }
            AgentTurnStartMutation::LegacyDuplicate => {
                legacy_revision.ok_or(StorageError::InvalidActivity)
            }
        },
    )?;
    let persisted = store
        .read(|connection| read_turn(connection, projection.turn_id))?
        .ok_or(StorageError::AgentTurnNotFound)?;
    if receipt.replayed || legacy_replay.get() {
        Ok(AgentMutationOutcome::Duplicate(persisted))
    } else {
        Ok(AgentMutationOutcome::Applied(persisted))
    }
}
