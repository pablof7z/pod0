use std::path::{Path, PathBuf};

use pod0_application::{
    DurableExternalEffectRequest, DurableTranscriptHostObservation, EffectOutcome,
    ExternalEffectKind,
};
use pod0_domain::{
    ActivityCorrelationId, ActivityId, EffectAttemptId, EffectIntentId, EffectLeaseId,
    UnixTimestampMilliseconds,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::effect_outbox_model::{EffectLease, EffectOutboxError};
use crate::migration_db::{
    configure, open_connection, user_version, validate_current_database_identity,
};

const MIN_LEASE_MILLISECONDS: i64 = 1_000;
const MAX_LEASE_MILLISECONDS: i64 = 300_000;

#[derive(Clone, Debug)]
pub struct EffectOutbox {
    path: PathBuf,
}

impl EffectOutbox {
    pub fn open(path: &Path) -> Result<Self, EffectOutboxError> {
        let connection = current_connection(path, true)?;
        drop(connection);
        Ok(Self { path: path.into() })
    }

    pub fn claim_next(
        &self,
        attempt_id: EffectAttemptId,
        lease_id: EffectLeaseId,
        now: UnixTimestampMilliseconds,
        lease_duration_milliseconds: u32,
    ) -> Result<Option<EffectLease>, EffectOutboxError> {
        self.claim_next_with_identity(
            Some((attempt_id, lease_id)),
            now,
            lease_duration_milliseconds,
        )
    }

    pub fn claim_next_generated(
        &self,
        now: UnixTimestampMilliseconds,
        lease_duration_milliseconds: u32,
    ) -> Result<Option<EffectLease>, EffectOutboxError> {
        self.claim_next_with_identity(None, now, lease_duration_milliseconds)
    }

    pub fn effect_kind(
        &self,
        intent_id: EffectIntentId,
    ) -> Result<Option<ExternalEffectKind>, EffectOutboxError> {
        let connection = current_connection(&self.path, true)?;
        let code = connection
            .query_row(
                "SELECT effect_kind_code FROM pod0_effect_intents WHERE intent_id=?1",
                [intent_id.into_bytes().as_slice()],
                |row| row.get::<_, u8>(0),
            )
            .optional()
            .map_err(|_| EffectOutboxError::Storage)?;
        code.map(decode_effect_kind).transpose()
    }

    fn claim_next_with_identity(
        &self,
        identity: Option<(EffectAttemptId, EffectLeaseId)>,
        now: UnixTimestampMilliseconds,
        lease_duration_milliseconds: u32,
    ) -> Result<Option<EffectLease>, EffectOutboxError> {
        let duration = i64::from(lease_duration_milliseconds);
        if !(MIN_LEASE_MILLISECONDS..=MAX_LEASE_MILLISECONDS).contains(&duration) {
            return Err(EffectOutboxError::InvalidLeaseDuration);
        }
        let expires_at = now
            .value
            .checked_add(duration)
            .ok_or(EffectOutboxError::InvalidLeaseDuration)?;
        let mut connection = current_connection(&self.path, false)?;
        configure(&connection).map_err(|_| EffectOutboxError::Storage)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| EffectOutboxError::Storage)?;
        let row = transaction
            .query_row(
                "SELECT i.intent_id,i.authorizing_activity_id,i.correlation_id,i.fence,i.request_json \
                 FROM pod0_effect_intents i WHERE i.available_at_ms<=?1 AND (i.state_code=1 OR \
                 (i.state_code=2 AND NOT EXISTS(SELECT 1 FROM pod0_effect_attempts a \
                 WHERE a.intent_id=i.intent_id AND a.state_code=1 AND a.lease_expires_at_ms>?1))) \
                 ORDER BY i.available_at_ms,i.intent_id LIMIT 1",
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
        let Some((intent, activity, correlation, prior_fence, payload)) = row else {
            return Ok(None);
        };
        let fence = prior_fence
            .checked_add(1)
            .ok_or(EffectOutboxError::InvalidRecord)?;
        let (attempt_id, lease_id) = identity.unwrap_or_else(|| generated_ids(&intent, fence));
        let updated = transaction
            .execute(
                "UPDATE pod0_effect_intents SET state_code=2,fence=?1 \
                 WHERE intent_id=?2 AND fence=?3",
                params![fence, intent.as_slice(), prior_fence],
            )
            .map_err(|_| EffectOutboxError::Storage)?;
        if updated != 1 {
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
                    expires_at
                ],
            )
            .map_err(|_| EffectOutboxError::Storage)?;
        transaction
            .commit()
            .map_err(|_| EffectOutboxError::Storage)?;
        let request: DurableExternalEffectRequest =
            serde_json::from_str(&payload).map_err(|_| EffectOutboxError::InvalidRecord)?;
        let fence = u64::try_from(fence).map_err(|_| EffectOutboxError::InvalidRecord)?;
        Ok(Some(EffectLease {
            intent_id: EffectIntentId::from_bytes(id(&intent)?),
            attempt_id,
            lease_id,
            fence,
            authorizing_activity_id: ActivityId::from_bytes(id(&activity)?),
            correlation_id: ActivityCorrelationId::from_bytes(id(&correlation)?),
            subject: request.subject,
            episode_id: request.episode_id,
            request,
            expires_at: UnixTimestampMilliseconds::new(expires_at),
        }))
    }
}

fn decode_effect_kind(code: u8) -> Result<ExternalEffectKind, EffectOutboxError> {
    match code {
        1 => Ok(ExternalEffectKind::FeedNetwork),
        2 => Ok(ExternalEffectKind::Playback),
        3 => Ok(ExternalEffectKind::RecallProvider),
        4 => Ok(ExternalEffectKind::ChapterProvider),
        5 => Ok(ExternalEffectKind::Download),
        6 => Ok(ExternalEffectKind::Notification),
        7 => Ok(ExternalEffectKind::TranscriptProvider),
        8 => Ok(ExternalEffectKind::AgentProvider),
        9 => Ok(ExternalEffectKind::AgentApproval),
        10 => Ok(ExternalEffectKind::AgentCapability),
        11 => Ok(ExternalEffectKind::ScheduledAgentProvider),
        12 => Ok(ExternalEffectKind::CoreWake),
        13 => Ok(ExternalEffectKind::Filesystem),
        14 => Ok(ExternalEffectKind::Publication),
        _ => Err(EffectOutboxError::InvalidRecord),
    }
}

include!("effect_outbox_observation.rs");

#[path = "effect_outbox_agent_observation.rs"]
mod agent_observation;
pub(crate) use agent_observation::{
    stage_agent_approval_observation_in_transaction,
    stage_agent_capability_observation_in_transaction,
    stage_agent_model_observation_in_transaction,
};

fn generated_ids(intent: &[u8], fence: i64) -> (EffectAttemptId, EffectLeaseId) {
    use sha2::{Digest as _, Sha256};

    let derive = |label: &[u8]| {
        let mut hash = Sha256::new();
        hash.update(b"pod0/effect-lease/v1\0");
        hash.update(label);
        hash.update(intent);
        hash.update(fence.to_be_bytes());
        let digest: [u8; 32] = hash.finalize().into();
        <[u8; 16]>::try_from(&digest[..16]).expect("fixed digest prefix")
    };
    (
        EffectAttemptId::from_bytes(derive(b"attempt")),
        EffectLeaseId::from_bytes(derive(b"lease")),
    )
}

fn current_connection(path: &Path, read_only: bool) -> Result<Connection, EffectOutboxError> {
    let connection = open_connection(path, read_only).map_err(|_| EffectOutboxError::Storage)?;
    let version = user_version(&connection).map_err(|_| EffectOutboxError::Storage)?;
    validate_current_database_identity(&connection, version)
        .map_err(|_| EffectOutboxError::Storage)?;
    Ok(connection)
}

fn id(value: &[u8]) -> Result<[u8; 16], EffectOutboxError> {
    value
        .try_into()
        .map_err(|_| EffectOutboxError::InvalidRecord)
}
