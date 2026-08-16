use pod0_application::{
    EffectOutcome, PersistedEffectLeaseIdentity, ScheduledAgentExecutionObservation,
};
use rusqlite::{OptionalExtension, params};

use crate::effect_outbox_model::EffectOutboxError;

pub(crate) fn validate_scheduled_agent_lease_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    lease: PersistedEffectLeaseIdentity,
    input: &crate::ScheduledAgentObservationInput,
    occurrence_id: pod0_domain::ScheduledOccurrenceId,
) -> Result<(), EffectOutboxError> {
    let fence = i64::try_from(lease.fence).map_err(|_| EffectOutboxError::InvalidRecord)?;
    let valid: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pod0_effect_attempts a \
             JOIN pod0_effect_intents i ON i.intent_id=a.intent_id \
             JOIN pod0_scheduled_attempts s ON s.occurrence_id=i.subject_id \
             JOIN pod0_scheduled_occurrences o ON o.occurrence_id=i.subject_id \
             WHERE a.lease_id=?1 AND a.fence=?2 AND a.attempt_id=?3 AND a.intent_id=?4 \
             AND i.authorizing_activity_id=?5 AND i.correlation_id=?6 AND a.state_code=1 \
             AND (a.lease_expires_at_ms>=?7 OR (a.observed_at_ms IS NOT NULL \
             AND o.stage='host_accepted')) AND a.lease_expires_at_ms=?8 \
             AND i.effect_kind_code=11 AND i.subject_code=5 AND i.subject_id=?9 \
             AND s.request_id=?10 AND s.cancellation_id=?11 AND s.issued_revision=?12)",
            params![
                lease.lease_id.into_bytes().as_slice(),
                fence,
                lease.attempt_id.into_bytes().as_slice(),
                lease.intent_id.into_bytes().as_slice(),
                lease.authorizing_activity_id.into_bytes().as_slice(),
                lease.correlation_id.into_bytes().as_slice(),
                input.observed_at.value,
                lease.expires_at.value,
                occurrence_id.into_bytes().as_slice(),
                input.request_id.into_bytes().as_slice(),
                input.cancellation_id.into_bytes().as_slice(),
                i64::try_from(input.observed_request_revision.value)
                    .map_err(|_| EffectOutboxError::InvalidRecord)?,
            ],
            |row| row.get(0),
        )
        .map_err(|_| EffectOutboxError::Storage)?;
    valid.then_some(()).ok_or(EffectOutboxError::StaleLease)
}

pub(crate) fn stage_scheduled_agent_observation_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    lease: PersistedEffectLeaseIdentity,
    input: &crate::ScheduledAgentObservationInput,
    occurrence_id: pod0_domain::ScheduledOccurrenceId,
    outcome: EffectOutcome,
    terminal: bool,
) -> Result<(), EffectOutboxError> {
    let fence = i64::try_from(lease.fence).map_err(|_| EffectOutboxError::InvalidRecord)?;
    let observation_json =
        serde_json::to_string(&input.observation).map_err(|_| EffectOutboxError::InvalidRecord)?;
    let outcome_json =
        serde_json::to_string(&outcome).map_err(|_| EffectOutboxError::InvalidRecord)?;
    let row: Option<(i64, Option<String>)> = transaction
        .query_row(
            "SELECT a.state_code,a.observation_json FROM pod0_effect_attempts a \
             JOIN pod0_effect_intents i ON i.intent_id=a.intent_id \
             JOIN pod0_scheduled_attempts s ON s.occurrence_id=i.subject_id \
             JOIN pod0_scheduled_occurrences o ON o.occurrence_id=i.subject_id \
             WHERE a.lease_id=?1 AND a.fence=?2 AND a.attempt_id=?3 AND a.intent_id=?4 \
             AND i.authorizing_activity_id=?5 AND i.correlation_id=?6 \
             AND (a.lease_expires_at_ms>=?7 OR (a.observed_at_ms IS NOT NULL \
             AND o.stage='host_accepted')) AND a.lease_expires_at_ms=?8 \
             AND i.effect_kind_code=11 AND i.subject_code=5 AND i.subject_id=?9 \
             AND s.request_id=?10 AND s.cancellation_id=?11 AND s.issued_revision=?12",
            params![
                lease.lease_id.into_bytes().as_slice(),
                fence,
                lease.attempt_id.into_bytes().as_slice(),
                lease.intent_id.into_bytes().as_slice(),
                lease.authorizing_activity_id.into_bytes().as_slice(),
                lease.correlation_id.into_bytes().as_slice(),
                input.observed_at.value,
                lease.expires_at.value,
                occurrence_id.into_bytes().as_slice(),
                input.request_id.into_bytes().as_slice(),
                input.cancellation_id.into_bytes().as_slice(),
                i64::try_from(input.observed_request_revision.value)
                    .map_err(|_| EffectOutboxError::InvalidRecord)?,
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| EffectOutboxError::Storage)?;
    let Some((state, stored)) = row else {
        return Err(EffectOutboxError::StaleLease);
    };
    if stored.as_deref() == Some(&observation_json) {
        return Ok(());
    }
    if state != 1 {
        return Err(EffectOutboxError::StaleLease);
    }
    let next_state = if terminal { 2 } else { 1 };
    let changed = transaction
        .execute(
            "UPDATE pod0_effect_attempts SET state_code=?1,observed_at_ms=?2,\
             outcome_schema_version=1,outcome_json=?3,observation_schema_version=1,\
             observation_json=?4 WHERE lease_id=?5 AND fence=?6 AND state_code=1",
            params![
                next_state,
                input.observed_at.value,
                outcome_json,
                observation_json,
                lease.lease_id.into_bytes().as_slice(),
                fence,
            ],
        )
        .map_err(|_| EffectOutboxError::Storage)?;
    (changed == 1)
        .then_some(())
        .ok_or(EffectOutboxError::StaleLease)
}

pub(crate) fn scheduled_observation_is_terminal(
    observation: &ScheduledAgentExecutionObservation,
) -> bool {
    !matches!(
        observation,
        ScheduledAgentExecutionObservation::Accepted { .. }
    )
}
