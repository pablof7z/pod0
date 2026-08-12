use pod0_application::{
    AgentAuthorizationObservation, AgentEffectObservationActivityInput, AgentPublicationTransition,
    AgentWorkflowAcceptance, EffectOutcome, agent_authorization_id, plan_agent_effect_observation,
};
use pod0_domain::{AgentTurnId, CommandId, ContentDigest, StateRevision};
use rusqlite::OptionalExtension;
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::agent_store::{persist, read_turn};
use crate::{
    AgentApprovalObservationCommitInput, AgentApprovalObservationCommitOutcome, AgentAuditKind,
    AgentCommandContext, EffectOutboxError, StorageError, TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_agent_approval_observation(
    path: &std::path::Path,
    input: AgentApprovalObservationCommitInput,
) -> Result<AgentApprovalObservationCommitOutcome, StorageError> {
    let store = crate::AgentStore::open(path)?;
    let command_id = CommandId::from_bytes(input.observation.request_id.into_bytes());
    let fingerprint = observation_fingerprint(&input);
    let staged = input.observation.clone();
    let lease = input.lease;
    let planned_turn = std::cell::Cell::new(None);
    let receipt = TransitionCommit::open(path)?.commit_planned_with_transaction_hooks(
        TransitionIngress {
            kind: TransitionIngressKind::HostObservation,
            id: lease.attempt_id.into_bytes(),
            fingerprint,
        },
        input.committed_at,
        |transaction| {
            let turn_id = effect_turn(transaction, lease.intent_id)?;
            planned_turn.set(Some(turn_id));
            let before = read_turn(transaction, turn_id)?
                .ok_or(StorageError::AgentTurnNotFound)?;
            validate_request(&before, &input.observation)?;
            let before_revision = before.projection().revision;
            if before_revision.value == u64::MAX {
                return Err(StorageError::InvalidAgentState);
            }
            let mut after = before;
            let acceptance = after.authorize(AgentAuthorizationObservation {
                proposal_id: input.observation.proposal_id,
                proposal_digest: input.observation.proposal_digest,
                authority: after.projection().proposal.as_ref()
                    .ok_or(StorageError::InvalidAgentState)?.required_authority,
                authorization_id: agent_authorization_id(input.observation.request_id),
                decision: input.observation.decision,
                observed_at: input.observation.observed_at,
            });
            let replay = acceptance == AgentWorkflowAcceptance::Duplicate;
            if !replay && acceptance != AgentWorkflowAcceptance::Updated {
                return Err(StorageError::AgentTurnConflict);
            }
            let committed = if replay {
                StateRevision::new(before_revision.value + 1)
            } else {
                after.projection().revision
            };
            plan_agent_effect_observation(AgentEffectObservationActivityInput {
                command_id,
                request_id: input.observation.request_id,
                turn_id,
                current_revision: before_revision,
                committed_revision: committed,
                intent_id: lease.intent_id,
                attempt_id: lease.attempt_id,
                authorizing_activity_id: lease.authorizing_activity_id,
                correlation_id: lease.correlation_id,
                episode_id: None,
                outcome: EffectOutcome::Succeeded,
                transition: AgentPublicationTransition::ApprovalChanged,
                next_effect: None,
                advance_turn: !replay && after.projection().stage
                    == pod0_application::AgentTurnStage::Authorized,
            })
            .map(|plan| plan.map_mutation(|mutation| (mutation, after, replay)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction| {
            let turn_id = planned_turn.get().ok_or(StorageError::InvalidActivity)?;
            crate::effect_outbox::stage_agent_approval_observation_in_transaction(
                transaction,
                lease,
                turn_id,
                &staged,
            )
            .map_err(effect_error)
        },
        |transaction, expected, (_, after, replay)| {
            if replay {
                return Err(StorageError::AgentTurnConflict);
            }
            let turn_id = planned_turn.get().ok_or(StorageError::InvalidActivity)?;
            let current =
                read_turn(transaction, turn_id)?.ok_or(StorageError::AgentTurnNotFound)?;
            if current.projection().revision != expected {
                return Err(StorageError::RevisionConflict);
            }
            let outcome = persist(
                transaction,
                AgentCommandContext {
                    command_id,
                    command_fingerprint: fingerprint.into_bytes(),
                    observed_at: input.committed_at,
                },
                Some(expected),
                AgentAuditKind::AuthorizationObserved,
                &after,
            )?;
            Ok(outcome.state().projection().revision)
        },
        |transaction| {
            crate::effect_outbox::complete_host_observation_in_transaction(transaction, lease)
                .map_err(effect_error)
        },
    )?;
    let turn_id = match planned_turn.get() {
        Some(value) => value,
        None => store.read(|connection| effect_turn(connection, lease.intent_id))?,
    };
    let state = store
        .turn(turn_id)?
        .ok_or(StorageError::AgentTurnNotFound)?;
    Ok(AgentApprovalObservationCommitOutcome {
        state,
        replayed: receipt.replayed,
    })
}

fn validate_request(
    state: &pod0_application::AgentTurnState,
    observation: &pod0_application::DurableAgentApprovalHostObservation,
) -> Result<(), StorageError> {
    let projection = state.projection();
    let proposal = projection
        .proposal
        .as_ref()
        .ok_or(StorageError::AgentTurnConflict)?;
    let expected = pod0_application::agent_approval_request_id(
        projection.turn_id,
        proposal.proposal_id,
        proposal.proposal_digest,
    );
    if observation.request_id != expected
        || observation.cancellation_id != state.cancellation_id()
        || observation.turn_id != projection.turn_id
        || observation.proposal_id != proposal.proposal_id
        || observation.proposal_digest != proposal.proposal_digest
    {
        return Err(StorageError::AgentTurnConflict);
    }
    Ok(())
}

fn effect_turn(
    connection: &rusqlite::Connection,
    intent_id: pod0_domain::EffectIntentId,
) -> Result<AgentTurnId, StorageError> {
    let bytes: Option<Vec<u8>> = connection
            .query_row(
                "SELECT subject_id FROM pod0_effect_intents WHERE intent_id=?1 \
                 AND effect_kind_code=9 AND subject_code=4",
                [intent_id.into_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| StorageError::sqlite("read agent approval effect subject", error))?;
    bytes
        .map(|value| {
            value
                .try_into()
                .map(AgentTurnId::from_bytes)
                .map_err(|_| StorageError::InvalidActivity)
        })
        .transpose()?
        .ok_or(StorageError::AgentTurnNotFound)
}

fn observation_fingerprint(input: &AgentApprovalObservationCommitInput) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/agent/approval-effect-observation/v1");
    hash.update(input.lease.intent_id.into_bytes());
    hash.update(input.lease.attempt_id.into_bytes());
    hash.update(input.lease.lease_id.into_bytes());
    hash.update(input.lease.fence.to_be_bytes());
    hash.update(serde_json::to_vec(&input.observation).expect("typed durable observation"));
    ContentDigest::from_bytes(hash.finalize().into())
}

fn effect_error(error: EffectOutboxError) -> StorageError {
    match error {
        EffectOutboxError::StaleLease => StorageError::AgentTurnConflict,
        EffectOutboxError::InvalidRecord | EffectOutboxError::InvalidLeaseDuration => {
            StorageError::InvalidActivity
        }
        EffectOutboxError::Storage => StorageError::Sqlite {
            operation: "commit agent approval effect observation",
        },
    }
}
