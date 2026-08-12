impl EffectOutbox {
    pub fn stage_observation(
        &self,
        lease_id: EffectLeaseId,
        fence: u64,
        outcome: EffectOutcome,
        observed_at: UnixTimestampMilliseconds,
    ) -> Result<(), EffectOutboxError> {
        let fence = i64::try_from(fence).map_err(|_| EffectOutboxError::InvalidRecord)?;
        let payload =
            serde_json::to_string(&outcome).map_err(|_| EffectOutboxError::InvalidRecord)?;
        let mut connection = current_connection(&self.path, false)?;
        configure(&connection).map_err(|_| EffectOutboxError::Storage)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| EffectOutboxError::Storage)?;
        let intent: Option<Vec<u8>> = transaction
            .query_row(
                "SELECT intent_id FROM pod0_effect_attempts WHERE lease_id=?1 AND fence=?2 AND \
             state_code=1 AND lease_expires_at_ms>=?3",
                params![lease_id.into_bytes().as_slice(), fence, observed_at.value],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| EffectOutboxError::Storage)?;
        let Some(intent) = intent else {
            return Err(EffectOutboxError::StaleLease);
        };
        transaction
            .execute(
                "UPDATE pod0_effect_attempts SET state_code=2,observed_at_ms=?1,\
             outcome_schema_version=1,outcome_json=?2 WHERE lease_id=?3 AND fence=?4",
                params![
                    observed_at.value,
                    payload,
                    lease_id.into_bytes().as_slice(),
                    fence
                ],
            )
            .map_err(|_| EffectOutboxError::Storage)?;
        let changed = transaction
            .execute(
                "UPDATE pod0_effect_intents SET state_code=3 \
             WHERE intent_id=?1 AND state_code=2 AND fence=?2",
                params![intent, fence],
            )
            .map_err(|_| EffectOutboxError::Storage)?;
        if changed != 1 {
            return Err(EffectOutboxError::StaleLease);
        }
        transaction.commit().map_err(|_| EffectOutboxError::Storage)
    }
}

pub(crate) fn stage_host_observation_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    observation: &DurableTranscriptHostObservation,
    outcome: EffectOutcome,
) -> Result<(), EffectOutboxError> {
    let fence = i64::try_from(lease.fence).map_err(|_| EffectOutboxError::InvalidRecord)?;
    let observed_at = observation.observed_at.value;
    let outcome_json =
        serde_json::to_string(&outcome).map_err(|_| EffectOutboxError::InvalidRecord)?;
    let observation_json =
        serde_json::to_string(observation).map_err(|_| EffectOutboxError::InvalidRecord)?;
    let row: Option<(Vec<u8>, i64, Option<String>)> = transaction
        .query_row(
            "SELECT a.intent_id,a.state_code,a.observation_json FROM pod0_effect_attempts a \
             JOIN pod0_effect_intents i ON i.intent_id=a.intent_id \
             JOIN pod0_transcript_workflows w ON w.episode_id=i.episode_id \
             AND w.workflow_id=i.subject_id \
             WHERE a.lease_id=?1 AND a.fence=?2 AND a.attempt_id=?3 AND a.intent_id=?4 \
             AND i.authorizing_activity_id=?5 AND i.correlation_id=?6 \
             AND a.lease_expires_at_ms>=?7 AND a.lease_expires_at_ms=?8 \
             AND i.effect_kind_code=7 AND i.subject_code=6 AND w.request_id=?9 \
             AND w.cancellation_id=?10 AND w.issued_revision=?11",
            params![
                lease.lease_id.into_bytes().as_slice(),
                fence,
                lease.attempt_id.into_bytes().as_slice(),
                lease.intent_id.into_bytes().as_slice(),
                lease.authorizing_activity_id.into_bytes().as_slice(),
                lease.correlation_id.into_bytes().as_slice(),
                observed_at,
                lease.expires_at.value,
                observation.request_id.into_bytes().as_slice(),
                observation.cancellation_id.into_bytes().as_slice(),
                i64::try_from(observation.observed_request_revision.value)
                    .map_err(|_| EffectOutboxError::InvalidRecord)?,
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|_| EffectOutboxError::Storage)?;
    let Some((intent_id, state, stored)) = row else {
        return Err(EffectOutboxError::StaleLease);
    };
    if state == 2 && stored.as_deref() == Some(&observation_json) {
        return Ok(());
    }
    if state != 1 {
        return Err(EffectOutboxError::StaleLease);
    }
    let changed = transaction
        .execute(
            "UPDATE pod0_effect_attempts SET state_code=2,observed_at_ms=?1,\
             outcome_schema_version=1,outcome_json=?2,observation_schema_version=1,\
             observation_json=?3 WHERE lease_id=?4 AND fence=?5 AND state_code=1",
            params![
                observed_at,
                outcome_json,
                observation_json,
                lease.lease_id.into_bytes().as_slice(),
                fence
            ],
        )
        .map_err(|_| EffectOutboxError::Storage)?;
    if changed != 1 {
        return Err(EffectOutboxError::StaleLease);
    }
    let active: Option<i64> = transaction
        .query_row(
            "SELECT 1 FROM pod0_effect_intents WHERE intent_id=?1 AND state_code=2 AND fence=?2",
            params![intent_id, fence],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| EffectOutboxError::Storage)?;
    active.ok_or(EffectOutboxError::StaleLease).map(drop)
}

pub(crate) fn complete_host_observation_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
) -> Result<(), EffectOutboxError> {
    let fence = i64::try_from(lease.fence).map_err(|_| EffectOutboxError::InvalidRecord)?;
    let intent: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT intent_id FROM pod0_effect_attempts WHERE lease_id=?1 AND fence=?2 \
             AND attempt_id=?3 AND intent_id=?4 AND state_code=2 AND observation_json IS NOT NULL",
            params![
                lease.lease_id.into_bytes().as_slice(),
                fence,
                lease.attempt_id.into_bytes().as_slice(),
                lease.intent_id.into_bytes().as_slice(),
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| EffectOutboxError::Storage)?;
    let Some(intent) = intent else {
        return Err(EffectOutboxError::StaleLease);
    };
    let attempt_changed = transaction
        .execute(
            "UPDATE pod0_effect_attempts SET state_code=3 WHERE lease_id=?1 AND fence=?2 \
             AND state_code=2",
            params![lease.lease_id.into_bytes().as_slice(), fence],
        )
        .map_err(|_| EffectOutboxError::Storage)?;
    let intent_changed = transaction
        .execute(
            "UPDATE pod0_effect_intents SET state_code=3 WHERE intent_id=?1 AND state_code=2 \
             AND fence=?2",
            params![intent, fence],
        )
        .map_err(|_| EffectOutboxError::Storage)?;
    if attempt_changed != 1 || intent_changed != 1 {
        return Err(EffectOutboxError::StaleLease);
    }
    Ok(())
}

pub(crate) fn stage_recall_observation_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    observation: &pod0_application::DurableRecallHostObservation,
) -> Result<(), EffectOutboxError> {
    let fence = i64::try_from(lease.fence).map_err(|_| EffectOutboxError::InvalidRecord)?;
    let observation_json =
        serde_json::to_string(observation).map_err(|_| EffectOutboxError::InvalidRecord)?;
    let outcome_json = serde_json::to_string(&EffectOutcome::Succeeded)
        .map_err(|_| EffectOutboxError::InvalidRecord)?;
    let row: Option<(i64, Option<String>)> = transaction
        .query_row(
            "SELECT a.state_code,a.observation_json FROM pod0_effect_attempts a \
             JOIN pod0_effect_intents i ON i.intent_id=a.intent_id \
             JOIN pod0_evidence_selection s ON s.episode_id=i.episode_id \
             WHERE a.lease_id=?1 AND a.fence=?2 AND a.attempt_id=?3 AND a.intent_id=?4 \
             AND i.authorizing_activity_id=?5 AND i.correlation_id=?6 \
             AND a.lease_expires_at_ms>=?7 AND a.lease_expires_at_ms=?8 \
             AND i.effect_kind_code=3 AND i.subject_code=2 AND i.subject_id=?9 \
             AND s.generation_id=?10",
            params![
                lease.lease_id.into_bytes().as_slice(),
                fence,
                lease.attempt_id.into_bytes().as_slice(),
                lease.intent_id.into_bytes().as_slice(),
                lease.authorizing_activity_id.into_bytes().as_slice(),
                lease.correlation_id.into_bytes().as_slice(),
                observation.observed_at.value,
                lease.expires_at.value,
                observation.episode_id.into_bytes().as_slice(),
                observation.generation_id.into_bytes().as_slice(),
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| EffectOutboxError::Storage)?;
    let Some((state, stored)) = row else {
        return Err(EffectOutboxError::StaleLease);
    };
    if state == 2 && stored.as_deref() == Some(&observation_json) {
        return Ok(());
    }
    if state != 1 {
        return Err(EffectOutboxError::StaleLease);
    }
    let changed = transaction
        .execute(
            "UPDATE pod0_effect_attempts SET state_code=2,observed_at_ms=?1,\
             outcome_schema_version=1,outcome_json=?2,observation_schema_version=1,\
             observation_json=?3 WHERE lease_id=?4 AND fence=?5 AND state_code=1",
            params![
                observation.observed_at.value,
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
