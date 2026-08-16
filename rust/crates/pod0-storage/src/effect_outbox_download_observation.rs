pub(crate) fn stage_download_observation_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    observation: &pod0_application::HostObservationEnvelope,
    fingerprint: pod0_domain::ContentDigest,
    outcome: pod0_application::EffectOutcome,
) -> Result<(), EffectOutboxError> {
    let fence = i64::try_from(lease.fence).map_err(|_| EffectOutboxError::InvalidRecord)?;
    let observation_json = crate::transcript_import_digest::hex_digest(fingerprint);
    let outcome_json =
        serde_json::to_string(&outcome).map_err(|_| EffectOutboxError::InvalidRecord)?;
    let row: Option<(i64, Option<String>)> = transaction
        .query_row(
            "SELECT a.state_code,a.observation_json FROM pod0_effect_attempts a \
             JOIN pod0_effect_intents i ON i.intent_id=a.intent_id \
             JOIN pod0_download_host_requests r ON r.episode_id=i.episode_id \
             WHERE a.lease_id=?1 AND a.fence=?2 AND a.attempt_id=?3 AND a.intent_id=?4 \
             AND i.authorizing_activity_id=?5 AND i.correlation_id=?6 \
             AND a.lease_expires_at_ms>=?7 AND a.lease_expires_at_ms=?8 \
             AND i.effect_kind_code=5 AND i.subject_code=2 AND r.request_id=?9",
            params![
                lease.lease_id.into_bytes().as_slice(),
                fence,
                lease.attempt_id.into_bytes().as_slice(),
                lease.intent_id.into_bytes().as_slice(),
                lease.authorizing_activity_id.into_bytes().as_slice(),
                lease.correlation_id.into_bytes().as_slice(),
                observation.observed_at.value,
                lease.expires_at.value,
                observation.request_id.into_bytes().as_slice(),
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

pub(crate) fn validate_download_observation_lease_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    observation: &pod0_application::HostObservationEnvelope,
) -> Result<(), EffectOutboxError> {
    let fence = i64::try_from(lease.fence).map_err(|_| EffectOutboxError::InvalidRecord)?;
    let found: Option<u8> = transaction
        .query_row(
            "SELECT 1 FROM pod0_effect_attempts a \
             JOIN pod0_effect_intents i ON i.intent_id=a.intent_id \
             JOIN pod0_download_host_requests r ON r.episode_id=i.episode_id \
             WHERE a.lease_id=?1 AND a.fence=?2 AND a.attempt_id=?3 AND a.intent_id=?4 \
             AND i.authorizing_activity_id=?5 AND i.correlation_id=?6 \
             AND a.lease_expires_at_ms>=?7 AND a.lease_expires_at_ms=?8 \
             AND a.state_code=1 AND i.state_code=2 AND i.effect_kind_code=5 \
             AND i.subject_code=2 AND r.request_id=?9",
            params![
                lease.lease_id.into_bytes().as_slice(),
                fence,
                lease.attempt_id.into_bytes().as_slice(),
                lease.intent_id.into_bytes().as_slice(),
                lease.authorizing_activity_id.into_bytes().as_slice(),
                lease.correlation_id.into_bytes().as_slice(),
                observation.observed_at.value,
                lease.expires_at.value,
                observation.request_id.into_bytes().as_slice(),
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| EffectOutboxError::Storage)?;
    found.map(|_| ()).ok_or(EffectOutboxError::StaleLease)
}
