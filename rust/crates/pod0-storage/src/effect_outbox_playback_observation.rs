use pod0_application::{
    DurableEffectExecution, HostObservationEnvelope, PersistedEffectLeaseIdentity,
};
use pod0_domain::ContentDigest;

#[derive(serde::Serialize, serde::Deserialize)]
struct PlaybackProgressMarker {
    sequence_number: u64,
    fingerprint: String,
}

impl EffectOutbox {
    pub fn accept_transient_playback_observation(
        &self,
        lease: PersistedEffectLeaseIdentity,
        observation: &HostObservationEnvelope,
        fingerprint: ContentDigest,
    ) -> Result<(), EffectOutboxError> {
        let mut connection = current_connection(&self.path, false)?;
        configure(&connection).map_err(|_| EffectOutboxError::Storage)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| EffectOutboxError::Storage)?;
        record_playback_progress_in_transaction(
            &transaction,
            lease,
            observation,
            fingerprint,
        )?;
        transaction
            .commit()
            .map_err(|_| EffectOutboxError::Storage)
    }
}

pub(crate) fn validate_playback_observation_lease_in_transaction(
    connection: &rusqlite::Connection,
    lease: PersistedEffectLeaseIdentity,
    observation: &HostObservationEnvelope,
) -> Result<(), super::EffectOutboxError> {
    let fence = i64::try_from(lease.fence)
        .map_err(|_| super::EffectOutboxError::InvalidRecord)?;
    let payload: Option<String> = connection
        .query_row(
            "SELECT i.request_json FROM pod0_effect_attempts a \
             JOIN pod0_effect_intents i ON i.intent_id=a.intent_id \
             WHERE a.lease_id=?1 AND a.fence=?2 AND a.attempt_id=?3 AND a.intent_id=?4 \
             AND i.authorizing_activity_id=?5 AND i.correlation_id=?6 \
             AND a.lease_expires_at_ms=?7 AND a.state_code=1 AND i.state_code=2 \
             AND i.fence=a.fence AND i.effect_kind_code=2",
            params![
                lease.lease_id.into_bytes().as_slice(),
                fence,
                lease.attempt_id.into_bytes().as_slice(),
                lease.intent_id.into_bytes().as_slice(),
                lease.authorizing_activity_id.into_bytes().as_slice(),
                lease.correlation_id.into_bytes().as_slice(),
                lease.expires_at.value,
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| super::EffectOutboxError::Storage)?;
    let request: DurableExternalEffectRequest = serde_json::from_str(
        payload
            .as_deref()
            .ok_or(super::EffectOutboxError::StaleLease)?,
    )
    .map_err(|_| super::EffectOutboxError::InvalidRecord)?;
    let DurableEffectExecution::Playback { request } = request.execution else {
        return Err(super::EffectOutboxError::InvalidRecord);
    };
    let exact = request.to_host();
    if exact.request_id != observation.request_id
        || exact.cancellation_id != observation.cancellation_id
        || exact.issued_revision != observation.observed_request_revision
    {
        return Err(super::EffectOutboxError::StaleLease);
    }
    let stream_started_at: Option<i64> = connection
        .query_row(
            "SELECT observed_at_ms FROM pod0_effect_attempts WHERE attempt_id=?1",
            [lease.attempt_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|_| super::EffectOutboxError::Storage)?;
    let is_stream = matches!(
        request.action,
        pod0_application::DurablePlaybackEffectAction::ObservePlayback { .. }
    );
    if observation.observed_at.value > lease.expires_at.value
        && !(is_stream && stream_started_at.is_some())
    {
        return Err(super::EffectOutboxError::StaleLease);
    }
    Ok(())
}

pub(crate) fn record_playback_progress_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    lease: PersistedEffectLeaseIdentity,
    observation: &HostObservationEnvelope,
    fingerprint: ContentDigest,
) -> Result<(), super::EffectOutboxError> {
    validate_playback_observation_lease_in_transaction(transaction, lease, observation)?;
    let stored: Option<String> = transaction
        .query_row(
            "SELECT observation_json FROM pod0_effect_attempts WHERE attempt_id=?1 AND state_code=1",
            [lease.attempt_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|_| super::EffectOutboxError::Storage)?;
    if stored
        .as_deref()
        .and_then(|value| serde_json::from_str::<PlaybackProgressMarker>(value).ok())
        .is_some_and(|marker| marker.sequence_number >= observation.sequence_number)
    {
        return Err(super::EffectOutboxError::StaleLease);
    }
    let marker = serde_json::to_string(&PlaybackProgressMarker {
        sequence_number: observation.sequence_number,
        fingerprint: crate::transcript_import_digest::hex_digest(fingerprint),
    })
    .map_err(|_| super::EffectOutboxError::InvalidRecord)?;
    let outcome = serde_json::to_string(&EffectOutcome::Progressed)
        .map_err(|_| super::EffectOutboxError::InvalidRecord)?;
    let changed = transaction
        .execute(
            "UPDATE pod0_effect_attempts SET observed_at_ms=COALESCE(observed_at_ms,?1),\
             outcome_schema_version=1,outcome_json=?2,observation_schema_version=1,\
             observation_json=?3 WHERE attempt_id=?4 AND state_code=1",
            params![
                observation.observed_at.value,
                outcome,
                marker,
                lease.attempt_id.into_bytes().as_slice(),
            ],
        )
        .map_err(|_| super::EffectOutboxError::Storage)?;
    (changed == 1)
        .then_some(())
        .ok_or(super::EffectOutboxError::StaleLease)
}

pub(crate) fn stage_playback_observation_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    lease: PersistedEffectLeaseIdentity,
    observation: &HostObservationEnvelope,
    fingerprint: ContentDigest,
    outcome: EffectOutcome,
) -> Result<(), super::EffectOutboxError> {
    record_playback_progress_in_transaction(transaction, lease, observation, fingerprint)?;
    let fence = i64::try_from(lease.fence)
        .map_err(|_| super::EffectOutboxError::InvalidRecord)?;
    let outcome_json =
        serde_json::to_string(&outcome).map_err(|_| super::EffectOutboxError::InvalidRecord)?;
    let changed = transaction
        .execute(
            "UPDATE pod0_effect_attempts SET state_code=2,observed_at_ms=?1,\
             outcome_schema_version=1,outcome_json=?2,observation_schema_version=1,\
             observation_json=observation_json WHERE lease_id=?3 AND fence=?4 AND state_code=1",
            params![
                observation.observed_at.value,
                outcome_json,
                lease.lease_id.into_bytes().as_slice(),
                fence,
            ],
        )
        .map_err(|_| super::EffectOutboxError::Storage)?;
    (changed == 1)
        .then_some(())
        .ok_or(super::EffectOutboxError::StaleLease)
}
