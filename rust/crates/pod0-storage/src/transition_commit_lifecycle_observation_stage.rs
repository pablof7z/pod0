fn stage_observation(
    transaction: &Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    observation: &DurableLifecycleHostObservation,
) -> Result<(), StorageError> {
    let fence = i64::try_from(lease.fence).map_err(|_| StorageError::InvalidActivity)?;
    let payload = serde_json::to_string(observation).map_err(|_| StorageError::InvalidActivity)?;
    let row: Option<(i64, Option<String>, String)> = transaction
        .query_row(
            "SELECT a.state_code,a.observation_json,i.request_json FROM pod0_effect_attempts a \
             JOIN pod0_effect_intents i ON i.intent_id=a.intent_id WHERE a.lease_id=?1 \
             AND a.fence=?2 AND a.attempt_id=?3 AND a.intent_id=?4 \
             AND i.authorizing_activity_id=?5 AND i.correlation_id=?6 \
             AND a.lease_expires_at_ms>=?7 AND a.lease_expires_at_ms=?8 \
             AND i.effect_kind_code=12",
            params![
                lease.lease_id.into_bytes().as_slice(),
                fence,
                lease.attempt_id.into_bytes().as_slice(),
                lease.intent_id.into_bytes().as_slice(),
                lease.authorizing_activity_id.into_bytes().as_slice(),
                lease.correlation_id.into_bytes().as_slice(),
                observation.observed_at.value,
                lease.expires_at.value,
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("validate lifecycle wake lease", error))?;
    let Some((state, stored, request_json)) = row else {
        return Err(StorageError::CommandConflict);
    };
    if state == 2 && stored.as_deref() == Some(&payload) {
        return Ok(());
    }
    let effect: pod0_application::DurableExternalEffectRequest =
        serde_json::from_str(&request_json).map_err(|_| StorageError::InvalidActivity)?;
    let DurableEffectExecution::Lifecycle { request } = effect.execution else {
        return Err(StorageError::CommandConflict);
    };
    let outcome_matches = match observation.outcome {
        LifecycleWakeOutcome::Reached { reason } => {
            reason == request.reason && observation.observed_at.value >= request.wake_at.value
        }
        LifecycleWakeOutcome::Failed { .. } | LifecycleWakeOutcome::Cancelled => true,
    };
    if state != 1
        || !outcome_matches
        || request.request_id != observation.request_id
        || request.cancellation_id != observation.cancellation_id
        || request.issued_revision != observation.observed_request_revision
    {
        return Err(StorageError::CommandConflict);
    }
    let outcome = serde_json::to_string(&effect_outcome(observation.outcome))
        .map_err(|_| StorageError::InvalidActivity)?;
    let changed = transaction
        .execute(
            "UPDATE pod0_effect_attempts SET state_code=2,observed_at_ms=?1,\
             outcome_schema_version=1,outcome_json=?2,observation_schema_version=1,\
             observation_json=?3 WHERE lease_id=?4 AND fence=?5 AND state_code=1",
            params![
                observation.observed_at.value,
                outcome,
                payload,
                lease.lease_id.into_bytes().as_slice(),
                fence,
            ],
        )
        .map_err(|error| StorageError::sqlite("stage lifecycle wake observation", error))?;
    (changed == 1)
        .then_some(())
        .ok_or(StorageError::CommandConflict)
}

fn observation_identity(attempt: EffectAttemptId, sequence: u64) -> EffectAttemptId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-lifecycle-wake-observation-v1\0");
    hash.update(attempt.into_bytes());
    hash.update(sequence.to_be_bytes());
    EffectAttemptId::from_bytes(hash.finalize()[..16].try_into().expect("digest"))
}

fn observation_fingerprint(
    observation: &DurableLifecycleHostObservation,
) -> Result<ContentDigest, StorageError> {
    let bytes = serde_json::to_vec(observation).map_err(|_| StorageError::InvalidActivity)?;
    Ok(ContentDigest::from_bytes(Sha256::digest(bytes).into()))
}
