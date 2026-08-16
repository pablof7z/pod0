fn stage_feed_observation(
    transaction: &Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    observation: &pod0_application::DurableFeedHostObservation,
) -> Result<(), StorageError> {
    let fence = i64::try_from(lease.fence).map_err(|_| StorageError::InvalidActivity)?;
    let payload = serde_json::to_string(observation).map_err(|_| StorageError::InvalidActivity)?;
    let state: Option<(i64, Option<String>)> = transaction
        .query_row(
            "SELECT a.state_code,a.observation_json FROM pod0_effect_attempts a \
         JOIN pod0_effect_intents i ON i.intent_id=a.intent_id JOIN pod0_feed_fetch_workflows w \
         ON w.request_id=?1 AND w.podcast_id=i.subject_id WHERE a.lease_id=?2 AND a.fence=?3 \
         AND a.attempt_id=?4 AND a.intent_id=?5 AND i.authorizing_activity_id=?6 \
         AND i.correlation_id=?7 AND a.lease_expires_at_ms>=?8 AND a.lease_expires_at_ms=?9 \
         AND i.effect_kind_code=1 AND i.subject_code=1 AND w.cancellation_id=?10 \
         AND w.issued_revision=?11",
            params![
                observation.request_id.into_bytes().as_slice(),
                lease.lease_id.into_bytes().as_slice(),
                fence,
                lease.attempt_id.into_bytes().as_slice(),
                lease.intent_id.into_bytes().as_slice(),
                lease.authorizing_activity_id.into_bytes().as_slice(),
                lease.correlation_id.into_bytes().as_slice(),
                observation.observed_at.value,
                lease.expires_at.value,
                observation.cancellation_id.into_bytes().as_slice(),
                i64::try_from(observation.observed_request_revision.value)
                    .map_err(|_| StorageError::InvalidActivity)?
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("validate feed observation lease", error))?;
    if state
        .as_ref()
        .is_some_and(|(code, stored)| *code == 2 && stored.as_deref() == Some(&payload))
    {
        return Ok(());
    }
    if !state.is_some_and(|(code, _)| code == 1) {
        return Err(StorageError::CommandConflict);
    }
    let outcome = serde_json::to_string(&EffectOutcome::Succeeded)
        .map_err(|_| StorageError::InvalidActivity)?;
    let changed = transaction.execute(
        "UPDATE pod0_effect_attempts SET state_code=2,observed_at_ms=?1,outcome_schema_version=1,\
         outcome_json=?2,observation_schema_version=1,observation_json=?3 WHERE lease_id=?4 \
         AND fence=?5 AND state_code=1",
        params![observation.observed_at.value,outcome,payload,lease.lease_id.into_bytes().as_slice(),fence],
    ).map_err(|error| StorageError::sqlite("stage feed observation", error))?;
    (changed == 1)
        .then_some(())
        .ok_or(StorageError::CommandConflict)
}

fn complete_feed_observation(
    transaction: &Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
) -> Result<(), StorageError> {
    crate::effect_outbox::complete_host_observation_in_transaction(transaction, lease)
        .map_err(|_| StorageError::CommandConflict)
}

fn feed_effect_outcome(action: &FeedFetchLeasedObservationAction) -> EffectOutcome {
    match action {
        FeedFetchLeasedObservationAction::Apply { .. }
        | FeedFetchLeasedObservationAction::NotModified { .. } => EffectOutcome::Succeeded,
        FeedFetchLeasedObservationAction::Cancel => EffectOutcome::Cancelled,
        FeedFetchLeasedObservationAction::Fail { failure_code, .. } => EffectOutcome::Failed {
            code: match failure_code.as_str() {
                "offline" => ActivityFailureCode::Offline,
                "timed_out" => ActivityFailureCode::TimedOut,
                "permission_denied" => ActivityFailureCode::PermissionDenied,
                "response_too_large" => ActivityFailureCode::ResponseTooLarge,
                _ => ActivityFailureCode::InvalidResponse,
            },
        },
    }
}

fn feed_observation_identity(
    attempt: pod0_domain::EffectAttemptId,
    sequence: u64,
) -> pod0_domain::EffectAttemptId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-feed-observation-v1\0");
    hash.update(attempt.into_bytes());
    hash.update(sequence.to_be_bytes());
    pod0_domain::EffectAttemptId::from_bytes(hash.finalize()[..16].try_into().expect("digest"))
}

fn feed_observation_fingerprint(
    input: &FeedFetchObservationCommitInput,
) -> Result<ContentDigest, StorageError> {
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(&input.observation).map_err(|_| StorageError::InvalidActivity)?);
    Ok(ContentDigest::from_bytes(hash.finalize().into()))
}
