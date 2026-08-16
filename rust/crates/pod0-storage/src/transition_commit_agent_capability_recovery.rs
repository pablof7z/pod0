use pod0_application::{
    AgentCapabilityExecutionMode, AgentCapabilityRecoveryActivityInput, DurableEffectExecution,
    DurableExternalEffectRequest, EffectOutcome, plan_agent_capability_recovery,
};
use pod0_domain::{CommandId, ContentDigest, HostRequestId, UnixTimestampMilliseconds};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::agent_store::{persist, read_turn};
use crate::{AgentAuditKind, AgentCommandContext, StorageError, TransitionIngress, TransitionIngressKind};

struct Candidate {
    intent_id: pod0_domain::EffectIntentId,
    attempt_id: pod0_domain::EffectAttemptId,
    authorizing_activity_id: pod0_domain::ActivityId,
    correlation_id: pod0_domain::ActivityCorrelationId,
    request: pod0_application::DurableAgentCapabilityEffectRequest,
}

pub(crate) fn commit_expired_agent_capability_recovery(
    path: &std::path::Path,
    now: UnixTimestampMilliseconds,
) -> Result<bool, StorageError> {
    let candidate = crate::AgentStore::open(path)?.read(|connection| candidate(connection, now))?;
    let Some(candidate) = candidate else { return Ok(false) };
    let recovery_id = CommandId::from_bytes(candidate.attempt_id.into_bytes());
    let fingerprint = recovery_fingerprint(&candidate);
    let recovered = std::cell::Cell::new(false);
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::Recovery,
            id: candidate.attempt_id.into_bytes(),
            fingerprint,
        },
        now,
        |transaction| {
            let turn_id = candidate.request.capability.turn_id;
            let mut state = read_turn(transaction, turn_id)?
                .ok_or(StorageError::AgentTurnNotFound)?;
            let recoverable = candidate.request.capability.generated_audio_target.is_some();
            let recovery = recoverable.then(|| recovery_request(&candidate.request, recovery_id, now));
            recovered.set(recoverable);
            let current_revision = state.projection().revision;
            let after = if recoverable {
                None
            } else {
                if state.mark_outcome_ambiguous(now) != pod0_application::AgentWorkflowAcceptance::Updated {
                    return Err(StorageError::AgentTurnConflict);
                }
                Some(state)
            };
            let committed_revision = after.as_ref().map_or(current_revision, |value| value.projection().revision);
            plan_agent_capability_recovery(AgentCapabilityRecoveryActivityInput {
                recovery_id,
                original_intent_id: candidate.intent_id,
                original_attempt_id: candidate.attempt_id,
                original_authorizing_activity_id: candidate.authorizing_activity_id,
                correlation_id: candidate.correlation_id,
                turn_id,
                current_revision,
                committed_revision,
                recovery,
            })
            .map(|plan| plan.map_mutation(|mutation| (mutation, after)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, (_, after)| {
            retire_original(transaction, &candidate, now)?;
            let Some(after) = after else { return Ok(expected) };
            let outcome = persist(
                transaction,
                AgentCommandContext {
                    command_id: recovery_id,
                    command_fingerprint: fingerprint.into_bytes(),
                    observed_at: now,
                },
                Some(expected),
                AgentAuditKind::Recovered,
                &after,
            )?;
            Ok(outcome.state().projection().revision)
        },
    )?;
    Ok(recovered.get())
}

fn candidate(
    connection: &rusqlite::Connection,
    now: UnixTimestampMilliseconds,
) -> Result<Option<Candidate>, StorageError> {
    let row: Option<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, String)> = connection
        .query_row(
            "SELECT i.intent_id,a.attempt_id,i.authorizing_activity_id,i.correlation_id,i.request_json \
             FROM pod0_effect_attempts a JOIN pod0_effect_intents i ON i.intent_id=a.intent_id \
             WHERE i.effect_kind_code=10 AND i.state_code=2 AND a.state_code=1 \
             AND json_extract(i.request_json,\
             '$.execution.AgentCapability.request.capability.execution_mode')='Perform' \
             AND a.lease_expires_at_ms<?1 ORDER BY a.lease_expires_at_ms,a.attempt_id LIMIT 1",
            [now.value],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read expired agent capability", error))?;
    row.map(|(intent, attempt, activity, correlation, payload)| {
        let durable: DurableExternalEffectRequest =
            serde_json::from_str(&payload).map_err(|_| StorageError::InvalidActivity)?;
        let DurableEffectExecution::AgentCapability { request } = durable.execution else {
            return Err(StorageError::InvalidActivity);
        };
        if request.capability.execution_mode != AgentCapabilityExecutionMode::Perform {
            return Err(StorageError::InvalidActivity);
        }
        Ok(Candidate {
            intent_id: pod0_domain::EffectIntentId::from_bytes(id(intent)?),
            attempt_id: pod0_domain::EffectAttemptId::from_bytes(id(attempt)?),
            authorizing_activity_id: pod0_domain::ActivityId::from_bytes(id(activity)?),
            correlation_id: pod0_domain::ActivityCorrelationId::from_bytes(id(correlation)?),
            request,
        })
    })
    .transpose()
}

fn recovery_request(
    original: &pod0_application::DurableAgentCapabilityEffectRequest,
    command_id: CommandId,
    now: UnixTimestampMilliseconds,
) -> pod0_application::DurableAgentCapabilityEffectRequest {
    let mut request = original.clone();
    request.request_id = recovery_request_id(original.request_id);
    request.command_id = command_id;
    request.deadline_at = Some(UnixTimestampMilliseconds::new(now.value.saturating_add(120_000)));
    request.capability.execution_mode = AgentCapabilityExecutionMode::RecoverExisting;
    request
}

fn retire_original(
    transaction: &rusqlite::Transaction<'_>,
    candidate: &Candidate,
    now: UnixTimestampMilliseconds,
) -> Result<(), StorageError> {
    let outcome = serde_json::to_string(&EffectOutcome::OutcomeUnknown)
        .map_err(|_| StorageError::InvalidActivity)?;
    let changed = transaction.execute(
        "UPDATE pod0_effect_attempts SET state_code=3,observed_at_ms=?1,\
         outcome_schema_version=1,outcome_json=?2 WHERE attempt_id=?3 AND state_code=1",
        params![now.value, outcome, candidate.attempt_id.into_bytes().as_slice()],
    ).map_err(|error| StorageError::sqlite("retire ambiguous agent capability attempt", error))?;
    if changed != 1 { return Err(StorageError::AgentTurnConflict); }
    transaction.execute(
        "UPDATE pod0_effect_intents SET state_code=3 WHERE intent_id=?1 AND state_code=2",
        [candidate.intent_id.into_bytes().as_slice()],
    ).map_err(|error| StorageError::sqlite("retire ambiguous agent capability intent", error))?;
    Ok(())
}

fn recovery_request_id(original: HostRequestId) -> HostRequestId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/agent-capability-recovery-request/v1");
    hash.update(original.into_bytes());
    HostRequestId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}

fn recovery_fingerprint(candidate: &Candidate) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/agent-capability-recovery/v1");
    hash.update(candidate.intent_id.into_bytes());
    hash.update(candidate.attempt_id.into_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}

fn id(bytes: Vec<u8>) -> Result<[u8; 16], StorageError> {
    bytes.try_into().map_err(|_| StorageError::InvalidActivity)
}
