use pod0_application::{
    AgentCapabilityRecoveryActivityInput, AgentPublicationTransition, AgentWorkflowAcceptance,
    DurableEffectExecution, DurableExternalEffectRequest, EffectOutcome,
    plan_agent_capability_recovery,
};
use pod0_domain::{CommandId, ContentDigest, UnixTimestampMilliseconds};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::agent_store::{persist, read_turn};
use crate::{
    AgentAuditKind, AgentCommandContext, StorageError, TransitionIngress, TransitionIngressKind,
};

struct Candidate {
    intent_id: pod0_domain::EffectIntentId,
    attempt_id: pod0_domain::EffectAttemptId,
    authorizing_activity_id: pod0_domain::ActivityId,
    correlation_id: pod0_domain::ActivityCorrelationId,
    turn_id: pod0_domain::AgentTurnId,
}

pub(crate) fn commit_expired_agent_model_recovery(
    path: &std::path::Path,
    now: UnixTimestampMilliseconds,
) -> Result<bool, StorageError> {
    let candidate = crate::AgentStore::open(path)?.read(|connection| candidate(connection, now))?;
    let Some(candidate) = candidate else {
        return Ok(false);
    };
    let recovery_id = CommandId::from_bytes(candidate.attempt_id.into_bytes());
    let fingerprint = recovery_fingerprint(&candidate);
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::Recovery,
            id: candidate.attempt_id.into_bytes(),
            fingerprint,
        },
        now,
        |transaction| {
            let mut state = read_turn(transaction, candidate.turn_id)?
                .ok_or(StorageError::AgentTurnNotFound)?;
            let current_revision = state.projection().revision;
            if state.mark_outcome_ambiguous(now) != AgentWorkflowAcceptance::Updated {
                return Err(StorageError::AgentTurnConflict);
            }
            let committed_revision = state.projection().revision;
            plan_agent_capability_recovery(AgentCapabilityRecoveryActivityInput {
                recovery_id,
                original_intent_id: candidate.intent_id,
                original_attempt_id: candidate.attempt_id,
                original_authorizing_activity_id: candidate.authorizing_activity_id,
                correlation_id: candidate.correlation_id,
                turn_id: candidate.turn_id,
                current_revision,
                committed_revision,
                transition: AgentPublicationTransition::TurnStateChanged,
                recovery: None,
            })
            .map(|plan| plan.map_mutation(|mutation| (mutation, state)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, (_, state)| {
            retire_original(transaction, &candidate, now)?;
            let outcome = persist(
                transaction,
                AgentCommandContext {
                    command_id: recovery_id,
                    command_fingerprint: fingerprint.into_bytes(),
                    observed_at: now,
                },
                Some(expected),
                AgentAuditKind::Recovered,
                &state,
            )?;
            Ok(outcome.state().projection().revision)
        },
    )?;
    Ok(true)
}

fn candidate(
    connection: &rusqlite::Connection,
    now: UnixTimestampMilliseconds,
) -> Result<Option<Candidate>, StorageError> {
    let row: Option<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, String)> = connection
        .query_row(
            "SELECT i.intent_id,a.attempt_id,i.authorizing_activity_id,i.correlation_id,\
             i.subject_id,i.request_json FROM pod0_effect_attempts a JOIN pod0_effect_intents i \
             ON i.intent_id=a.intent_id WHERE i.effect_kind_code=8 AND i.subject_code=4 \
             AND i.state_code=2 AND a.state_code=1 AND a.lease_expires_at_ms<=?1 \
             ORDER BY a.lease_expires_at_ms,a.attempt_id LIMIT 1",
            [now.value],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read expired agent model", error))?;
    row.map(|(intent, attempt, activity, correlation, turn, payload)| {
        let durable: DurableExternalEffectRequest =
            serde_json::from_str(&payload).map_err(|_| StorageError::InvalidActivity)?;
        if !matches!(durable.execution, DurableEffectExecution::AgentModel { .. }) {
            return Err(StorageError::InvalidActivity);
        }
        Ok(Candidate {
            intent_id: pod0_domain::EffectIntentId::from_bytes(id(intent)?),
            attempt_id: pod0_domain::EffectAttemptId::from_bytes(id(attempt)?),
            authorizing_activity_id: pod0_domain::ActivityId::from_bytes(id(activity)?),
            correlation_id: pod0_domain::ActivityCorrelationId::from_bytes(id(correlation)?),
            turn_id: pod0_domain::AgentTurnId::from_bytes(id(turn)?),
        })
    })
    .transpose()
}

fn retire_original(
    transaction: &rusqlite::Transaction<'_>,
    candidate: &Candidate,
    now: UnixTimestampMilliseconds,
) -> Result<(), StorageError> {
    let outcome = serde_json::to_string(&EffectOutcome::OutcomeUnknown)
        .map_err(|_| StorageError::InvalidActivity)?;
    let attempts = transaction
        .execute(
            "UPDATE pod0_effect_attempts SET state_code=3,observed_at_ms=?1,\
             outcome_schema_version=1,outcome_json=?2 WHERE attempt_id=?3 AND state_code=1",
            params![
                now.value,
                outcome,
                candidate.attempt_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("retire ambiguous agent model attempt", error))?;
    let intents = transaction
        .execute(
            "UPDATE pod0_effect_intents SET state_code=3 WHERE intent_id=?1 AND state_code=2",
            [candidate.intent_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("retire ambiguous agent model intent", error))?;
    if attempts != 1 || intents != 1 {
        return Err(StorageError::AgentTurnConflict);
    }
    Ok(())
}

fn recovery_fingerprint(candidate: &Candidate) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/agent-model-ambiguous-recovery/v1");
    hash.update(candidate.intent_id.into_bytes());
    hash.update(candidate.attempt_id.into_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}

fn id(bytes: Vec<u8>) -> Result<[u8; 16], StorageError> {
    bytes.try_into().map_err(|_| StorageError::InvalidActivity)
}
