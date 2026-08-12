use pod0_application::{
    AgentCancellationActivityInput, AgentCancellationMutation, AgentWorkflowAcceptance,
    RequestDisposition, RequestRejectionReason, plan_agent_cancellation,
};
use pod0_domain::{AgentTurnId, ContentDigest, StateRevision};
use rusqlite::params;

use super::TransitionCommit;
use crate::agent_store::{persist, read_turn};
use crate::{
    AgentAuditKind, AgentCancellationCommitOutcome, AgentCommandContext, StorageError,
    TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_agent_cancellation(
    path: &std::path::Path,
    context: AgentCommandContext,
    turn_id: AgentTurnId,
    expected_revision: StateRevision,
) -> Result<AgentCancellationCommitOutcome, StorageError> {
    let store = crate::AgentStore::open(path)?;
    let fingerprint = ContentDigest::from_bytes(context.command_fingerprint);
    let accepted = std::cell::Cell::new(false);
    let planned_cancellation = std::cell::Cell::new(None);
    let receipt = TransitionCommit::open(path)?.commit_planned_with_transaction_hooks(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: context.command_id.into_bytes(),
            fingerprint,
        },
        context.observed_at,
        |transaction| {
            let before = read_turn(transaction, turn_id)?;
            let current = before
                .as_ref()
                .map_or(StateRevision::INITIAL, |state| state.projection().revision);
            planned_cancellation.set(before.as_ref().map(|state| state.cancellation_id()));
            let (after, disposition) = decide(before, expected_revision, context.observed_at);
            accepted.set(disposition == RequestDisposition::Accepted);
            let committed = after
                .as_ref()
                .map_or(current, |state| state.projection().revision);
            plan_agent_cancellation(AgentCancellationActivityInput {
                command_id: context.command_id,
                turn_id,
                current_revision: current,
                committed_revision: committed,
                disposition,
            })
            .map(|plan| plan.map_mutation(|mutation| (mutation, after)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |_| Ok(()),
        |transaction, expected, (mutation, after)| match mutation {
            AgentCancellationMutation::None => Ok(expected),
            AgentCancellationMutation::Cancel => {
                let state = after.ok_or(StorageError::InvalidAgentState)?;
                let current =
                    read_turn(transaction, turn_id)?.ok_or(StorageError::AgentTurnNotFound)?;
                if current.projection().revision != expected {
                    return Err(StorageError::RevisionConflict);
                }
                let outcome = persist(
                    transaction,
                    context,
                    Some(expected),
                    AgentAuditKind::Cancelled,
                    &state,
                )?;
                Ok(outcome.state().projection().revision)
            }
        },
        |transaction| {
            if accepted.get() {
                supersede_turn_work(transaction, turn_id)?;
            }
            Ok(())
        },
    )?;
    let cancellation_id = match planned_cancellation.get() {
        Some(value) => Some(value),
        None => store.turn(turn_id)?.map(|state| state.cancellation_id()),
    };
    Ok(AgentCancellationCommitOutcome {
        disposition: receipt.disposition,
        cancellation_id,
        replayed: receipt.replayed,
    })
}

fn decide(
    state: Option<pod0_application::AgentTurnState>,
    expected_revision: StateRevision,
    observed_at: pod0_domain::UnixTimestampMilliseconds,
) -> (Option<pod0_application::AgentTurnState>, RequestDisposition) {
    let Some(mut state) = state else {
        return (
            None,
            RequestDisposition::Rejected {
                reason: RequestRejectionReason::MissingSubject,
            },
        );
    };
    if state.projection().revision != expected_revision {
        return (
            Some(state),
            RequestDisposition::Rejected {
                reason: RequestRejectionReason::RevisionConflict,
            },
        );
    }
    match state.cancel(observed_at) {
        AgentWorkflowAcceptance::Updated => (Some(state), RequestDisposition::Accepted),
        AgentWorkflowAcceptance::Duplicate => (Some(state), RequestDisposition::AlreadyComplete),
        AgentWorkflowAcceptance::Stale => (Some(state), RequestDisposition::Stale),
        AgentWorkflowAcceptance::Rejected => (
            Some(state),
            RequestDisposition::Rejected {
                reason: RequestRejectionReason::NotAllowed,
            },
        ),
    }
}

fn supersede_turn_work(
    transaction: &rusqlite::Transaction<'_>,
    turn_id: AgentTurnId,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_effect_attempts SET state_code=4 WHERE intent_id IN(
             SELECT intent_id FROM pod0_effect_intents WHERE subject_code=4 AND subject_id=?1
             AND state_code IN(1,2)) AND state_code IN(1,2)",
            [turn_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("supersede agent effect attempts", error))?;
    transaction
        .execute(
            "UPDATE pod0_effect_intents SET state_code=4 WHERE subject_code=4 AND subject_id=?1
             AND state_code IN(1,2)",
            [turn_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("supersede agent effect intents", error))?;
    transaction
        .execute(
            "UPDATE pod0_internal_command_intents SET state_code=3 WHERE subject_code=4
             AND subject_id=?1 AND state_code=1",
            params![turn_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("supersede agent internal commands", error))?;
    Ok(())
}
