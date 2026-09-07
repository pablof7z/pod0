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
            false,
        )
    }

    pub fn claim_next_generated(
        &self,
        now: UnixTimestampMilliseconds,
        lease_duration_milliseconds: u32,
    ) -> Result<Option<EffectLease>, EffectOutboxError> {
        self.claim_next_with_identity(None, now, lease_duration_milliseconds, u16::MAX, false)
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
            false,
        )
    }

    pub fn claim_next_generated_for_headless(
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
            true,
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

    pub fn pending_requests(
        &self,
        maximum_count: u16,
    ) -> Result<Vec<DurableExternalEffectRequest>, EffectOutboxError> {
        let connection = current_connection(&self.path, true)?;
        let mut statement = connection
            .prepare(
                "SELECT request_json FROM pod0_effect_intents \
                 WHERE effect_kind_code!=14 AND state_code IN(1,2) \
                 ORDER BY available_at_ms,committed_at_ms,rowid LIMIT ?1",
            )
            .map_err(|_| EffectOutboxError::Storage)?;
        let rows = statement
            .query_map([i64::from(maximum_count.clamp(1, 64))], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|_| EffectOutboxError::Storage)?;
        rows.map(|row| {
            let json = row.map_err(|_| EffectOutboxError::Storage)?;
            serde_json::from_str(&json).map_err(|_| EffectOutboxError::InvalidRecord)
        })
        .collect()
    }

    pub fn next_claim_at(
        &self,
        now: UnixTimestampMilliseconds,
    ) -> Result<Option<UnixTimestampMilliseconds>, EffectOutboxError> {
        let connection = current_connection(&self.path, true)?;
        let value = connection
            .query_row(
                "SELECT MIN(CASE WHEN i.state_code=1 THEN \
                 CASE WHEN i.effect_kind_code=12 THEN MAX(i.available_at_ms,COALESCE(\
                 json_extract(i.request_json,'$.execution.Lifecycle.request.wake_at.value'),\
                 i.available_at_ms)) ELSE i.available_at_ms END ELSE MAX(\
                 CASE WHEN i.effect_kind_code=12 THEN MAX(i.available_at_ms,COALESCE(\
                 json_extract(i.request_json,'$.execution.Lifecycle.request.wake_at.value'),\
                 i.available_at_ms)) ELSE i.available_at_ms END,COALESCE((SELECT \
                 MAX(a.lease_expires_at_ms) FROM pod0_effect_attempts a WHERE \
                 a.intent_id=i.intent_id AND a.state_code=1),?1)) END) \
                 FROM pod0_effect_intents i WHERE i.effect_kind_code!=14 \
                 AND i.state_code IN(1,2) AND (i.state_code=1 OR ((\
                 i.effect_kind_code NOT IN(7,8,10) AND NOT (i.effect_kind_code=4 AND \
                 json_extract(i.request_json,'$.kind')='ModelChapterProvider' AND \
                 json_type(i.request_json,'$.execution.ModelChapter.request.action.Execute') \
                 IS NOT NULL)) OR json_type(i.request_json,\
                 '$.execution.Transcript.request.capability.FetchPublisher') IS NOT NULL OR \
                 json_type(i.request_json,\
                 '$.execution.Transcript.request.capability.RecoverProvider') IS NOT NULL OR \
                 json_type(i.request_json,'$.execution.ModelChapter.request.action.Recover') \
                 IS NOT NULL OR json_extract(i.request_json,\
                 '$.execution.AgentCapability.request.capability.execution_mode')=\
                 'RecoverExisting')) AND NOT EXISTS(SELECT 1 FROM \
                 pod0_effect_attempts observed WHERE observed.intent_id=i.intent_id \
                 AND observed.state_code=1 AND observed.observed_at_ms IS NOT NULL AND \
                 (json_type(i.request_json,'$.execution.Playback.request.action.ObservePlayback') \
                 IS NOT NULL OR (i.effect_kind_code=11 AND EXISTS(SELECT 1 FROM \
                 pod0_scheduled_occurrences occurrence WHERE occurrence.occurrence_id=i.subject_id \
                 AND occurrence.stage='host_accepted'))))",
                [now.value],
                |row| row.get::<_, Option<i64>>(0),
            )
            .map_err(|_| EffectOutboxError::Storage)?;
        Ok(value.map(UnixTimestampMilliseconds::new))
    }
}

include!("effect_outbox_claim.rs");
include!("effect_outbox_observation.rs");
include!("effect_outbox_chapter_observation.rs");
include!("effect_outbox_download_observation.rs");
include!("effect_outbox_playback_observation.rs");

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
