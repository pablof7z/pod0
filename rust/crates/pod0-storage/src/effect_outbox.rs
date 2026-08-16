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
    pub(super) path: PathBuf,
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
            u16::MAX,
        )
    }

    pub fn claim_next_generated(
        &self,
        now: UnixTimestampMilliseconds,
        lease_duration_milliseconds: u32,
    ) -> Result<Option<EffectLease>, EffectOutboxError> {
        self.claim_next_with_identity(None, now, lease_duration_milliseconds, u16::MAX)
    }

    pub fn claim_next_generated_with_publisher_limit(
        &self,
        now: UnixTimestampMilliseconds,
        lease_duration_milliseconds: u32,
        maximum_active_publisher_chapters: u16,
    ) -> Result<Option<EffectLease>, EffectOutboxError> {
        self.claim_next_with_identity(
            None,
            now,
            lease_duration_milliseconds,
            maximum_active_publisher_chapters,
        )
    }

    pub fn effect_kind(
        &self,
        intent_id: EffectIntentId,
    ) -> Result<Option<ExternalEffectKind>, EffectOutboxError> {
        self.effect_request(intent_id)
            .map(|request| request.map(|value| value.kind))
    }

    pub fn effect_request(
        &self,
        intent_id: EffectIntentId,
    ) -> Result<Option<DurableExternalEffectRequest>, EffectOutboxError> {
        let connection = current_connection(&self.path, true)?;
        let request = connection
            .query_row(
                "SELECT request_json FROM pod0_effect_intents WHERE intent_id=?1",
                [intent_id.into_bytes().as_slice()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| EffectOutboxError::Storage)?;
        request
            .map(|value| {
                serde_json::from_str::<DurableExternalEffectRequest>(&value)
                    .map_err(|_| EffectOutboxError::InvalidRecord)
            })
            .transpose()
    }

    fn claim_next_with_identity(
        &self,
        identity: Option<(EffectAttemptId, EffectLeaseId)>,
        now: UnixTimestampMilliseconds,
        lease_duration_milliseconds: u32,
        maximum_active_publisher_chapters: u16,
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
                 FROM pod0_effect_intents i WHERE i.effect_kind_code!=14 \
                 AND (i.state_code=1 OR i.effect_kind_code!=10 OR json_extract(i.request_json,\
                 '$.execution.AgentCapability.request.capability.execution_mode')='RecoverExisting') \
                 AND i.available_at_ms<=?1 AND (i.state_code=1 OR \
                 (i.state_code=2 AND NOT EXISTS(SELECT 1 FROM pod0_effect_attempts a \
                 WHERE a.intent_id=i.intent_id AND a.state_code=1 AND \
                 (a.lease_expires_at_ms>?1 OR (a.observed_at_ms IS NOT NULL AND \
                 json_type(i.request_json,'$.execution.Playback.request.action.ObservePlayback') \
                 IS NOT NULL) OR (a.observed_at_ms IS NOT NULL AND i.effect_kind_code=11 AND \
                 EXISTS(SELECT 1 FROM pod0_scheduled_occurrences occurrence \
                 WHERE occurrence.occurrence_id=i.subject_id \
                 AND occurrence.stage='host_accepted')))))) \
                 AND (json_extract(i.request_json,'$.kind')!='PublisherChapterProvider' OR \
                 (SELECT COUNT(*) FROM pod0_effect_attempts active \
                  JOIN pod0_effect_intents owned ON owned.intent_id=active.intent_id \
                  WHERE active.state_code=1 AND active.lease_expires_at_ms>?1 \
                  AND json_extract(owned.request_json,'$.kind')='PublisherChapterProvider')<?2) \
                 ORDER BY i.available_at_ms,i.committed_at_ms,i.rowid LIMIT 1",
                params![now.value, i64::from(maximum_active_publisher_chapters)],
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

include!("effect_outbox_observation.rs");
include!("effect_outbox_chapter_observation.rs");
include!("effect_outbox_download_observation.rs");
include!("effect_outbox_playback_observation.rs");

#[path = "effect_outbox_publication.rs"]
mod publication;

#[path = "effect_outbox_agent_observation.rs"]
mod agent_observation;
pub(crate) use agent_observation::{
    stage_agent_approval_observation_in_transaction,
    stage_agent_capability_observation_in_transaction,
    stage_agent_model_observation_in_transaction,
};

#[path = "effect_outbox_scheduled_agent_observation.rs"]
mod scheduled_agent_observation;
pub(crate) use scheduled_agent_observation::{
    scheduled_observation_is_terminal, stage_scheduled_agent_observation_in_transaction,
    validate_scheduled_agent_lease_in_transaction,
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

pub(super) fn current_connection(
    path: &Path,
    read_only: bool,
) -> Result<Connection, EffectOutboxError> {
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
