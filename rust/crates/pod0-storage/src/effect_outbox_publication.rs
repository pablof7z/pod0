use pod0_application::{
    DurableEffectExecution, DurableExternalEffectRequest, PersistedEffectLeaseIdentity,
};
use pod0_domain::{
    ActivityCorrelationId, ActivityId, EffectAttemptId, EffectIntentId, EffectLeaseId,
    UnixTimestampMilliseconds,
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use crate::effect_outbox_model::{EffectOutboxError, PublicationEffectLease};
use crate::migration_db::configure;
use crate::{EffectOutbox, effect_outbox};

impl EffectOutbox {
    pub fn claim_next_publication(
        &self,
        now: UnixTimestampMilliseconds,
        lease_duration_milliseconds: u32,
    ) -> Result<Option<PublicationEffectLease>, EffectOutboxError> {
        let duration = i64::from(lease_duration_milliseconds);
        if !(1_000..=300_000).contains(&duration) {
            return Err(EffectOutboxError::InvalidLeaseDuration);
        }
        let expires_at = now
            .value
            .checked_add(duration)
            .ok_or(EffectOutboxError::InvalidLeaseDuration)?;
        let mut connection = effect_outbox::current_connection(&self.path, false)?;
        configure(&connection).map_err(|_| EffectOutboxError::Storage)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| EffectOutboxError::Storage)?;
        let row = transaction
            .query_row(
                "SELECT intent_id,authorizing_activity_id,correlation_id,fence,request_json \
                 FROM pod0_effect_intents WHERE effect_kind_code=14 AND available_at_ms<=?1 \
                 AND (state_code=1 OR (state_code=2 AND NOT EXISTS(SELECT 1 FROM \
                 pod0_effect_attempts a WHERE a.intent_id=pod0_effect_intents.intent_id \
                 AND a.state_code=1 AND (a.lease_expires_at_ms>?1 OR EXISTS(SELECT 1 FROM \
                 pod0_publications p WHERE p.publication_id=pod0_effect_intents.subject_id \
                 AND p.receipt_id IS NOT NULL))))) \
                 ORDER BY available_at_ms,committed_at_ms,rowid LIMIT 1",
                [now.value],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| EffectOutboxError::Storage)?;
        let Some((intent, activity, correlation, previous_fence, payload)) = row else {
            return Ok(None);
        };
        let fence = previous_fence
            .checked_add(1)
            .ok_or(EffectOutboxError::InvalidRecord)?;
        let (attempt_id, lease_id) = publication_lease_ids(&intent, fence);
        let changed = transaction
            .execute(
                "UPDATE pod0_effect_intents SET state_code=2,fence=?1 \
                 WHERE intent_id=?2 AND fence=?3",
                params![fence, intent.as_slice(), previous_fence],
            )
            .map_err(|_| EffectOutboxError::Storage)?;
        if changed != 1 {
            return Err(EffectOutboxError::StaleLease);
        }
        transaction
            .execute(
                "INSERT INTO pod0_effect_attempts(attempt_id,intent_id,lease_id,fence,state_code,\
                 claimed_at_ms,lease_expires_at_ms) VALUES(?1,?2,?3,?4,1,?5,?6)",
                params![
                    attempt_id.into_bytes().as_slice(),
                    intent.as_slice(),
                    lease_id.into_bytes().as_slice(),
                    fence,
                    now.value,
                    expires_at,
                ],
            )
            .map_err(|_| EffectOutboxError::Storage)?;
        transaction
            .commit()
            .map_err(|_| EffectOutboxError::Storage)?;
        let request: DurableExternalEffectRequest =
            serde_json::from_str(&payload).map_err(|_| EffectOutboxError::InvalidRecord)?;
        let DurableEffectExecution::Publication { draft } = request.execution else {
            return Err(EffectOutboxError::InvalidRecord);
        };
        Ok(Some(PublicationEffectLease {
            lease: PersistedEffectLeaseIdentity {
                intent_id: EffectIntentId::from_bytes(id(&intent)?),
                authorizing_activity_id: ActivityId::from_bytes(id(&activity)?),
                correlation_id: ActivityCorrelationId::from_bytes(id(&correlation)?),
                attempt_id,
                lease_id,
                fence: u64::try_from(fence).map_err(|_| EffectOutboxError::InvalidRecord)?,
                expires_at: UnixTimestampMilliseconds::new(expires_at),
            },
            draft,
        }))
    }

    pub fn active_publication_lease(
        &self,
        publication_id: pod0_domain::PublicationId,
    ) -> Result<Option<PersistedEffectLeaseIdentity>, EffectOutboxError> {
        let connection = effect_outbox::current_connection(&self.path, true)?;
        let row = connection
            .query_row(
                "SELECT i.intent_id,i.authorizing_activity_id,i.correlation_id,a.attempt_id,\
                 a.lease_id,a.fence,a.lease_expires_at_ms FROM pod0_effect_intents i \
                 JOIN pod0_effect_attempts a ON a.intent_id=i.intent_id \
                 WHERE i.effect_kind_code=14 AND i.subject_code=7 AND i.subject_id=?1 \
                 AND i.state_code=2 AND a.state_code=1 AND a.fence=i.fence \
                 ORDER BY a.claimed_at_ms DESC LIMIT 1",
                [publication_id.into_bytes().as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, Vec<u8>>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| EffectOutboxError::Storage)?;
        row.map(
            |(intent, activity, correlation, attempt, lease, fence, expires)| {
                Ok(PersistedEffectLeaseIdentity {
                    intent_id: EffectIntentId::from_bytes(id(&intent)?),
                    authorizing_activity_id: ActivityId::from_bytes(id(&activity)?),
                    correlation_id: ActivityCorrelationId::from_bytes(id(&correlation)?),
                    attempt_id: EffectAttemptId::from_bytes(id(&attempt)?),
                    lease_id: EffectLeaseId::from_bytes(id(&lease)?),
                    fence: u64::try_from(fence).map_err(|_| EffectOutboxError::InvalidRecord)?,
                    expires_at: UnixTimestampMilliseconds::new(expires),
                })
            },
        )
        .transpose()
    }
}

fn publication_lease_ids(intent: &[u8], fence: i64) -> (EffectAttemptId, EffectLeaseId) {
    use sha2::{Digest as _, Sha256};
    let derive = |label: &[u8]| {
        let mut hash = Sha256::new();
        hash.update(b"pod0/publication-effect-lease/v1\0");
        hash.update(label);
        hash.update(intent);
        hash.update(fence.to_be_bytes());
        let digest: [u8; 32] = hash.finalize().into();
        digest[..16].try_into().expect("fixed digest prefix")
    };
    (
        EffectAttemptId::from_bytes(derive(b"attempt")),
        EffectLeaseId::from_bytes(derive(b"lease")),
    )
}

fn id(value: &[u8]) -> Result<[u8; 16], EffectOutboxError> {
    value
        .try_into()
        .map_err(|_| EffectOutboxError::InvalidRecord)
}
