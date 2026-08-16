use pod0_application::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject, EffectOutcome,
    NonEmptyActivityFacts,
};
use pod0_domain::{
    ActivityCorrelationId, ActivityId, ActivityTransactionId, EffectIntentId, EpisodeId,
    UnixTimestampMilliseconds,
};
use rusqlite::params;
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::StorageError;

pub(crate) fn append_v40_legacy_recovery_facts(
    transaction: &rusqlite::Transaction<'_>,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let rows = legacy_rows(transaction)?;
    for row in rows {
        let (activity_id, transaction_id) = identities(row.intent_id);
        let subject = row
            .episode_id
            .map_or(ActivitySubject::Global, |episode_id| {
                ActivitySubject::Episode { episode_id }
            });
        let fact = ActivityFactDraft {
            activity_id,
            transaction_id,
            correlation_id: row.correlation_id,
            caused_by_activity_id: Some(row.authorizing_activity_id),
            command_id: None,
            host_request_id: None,
            actor: ActivityActor::Recovery,
            origin: ActivityOrigin::Migration,
            subject,
            episode_id: row.episode_id,
            fact: ActivityFact::RecoveryTransition {
                outcome: EffectOutcome::OutcomeUnknown,
            },
        };
        TransitionCommit::append_migration_facts(
            transaction,
            &NonEmptyActivityFacts::new(fact),
            UnixTimestampMilliseconds::new(observed_at_ms),
        )?;
        transaction
            .execute(
                "UPDATE pod0_legacy_effect_recovery_v40 SET recovery_activity_id=?1
             WHERE intent_id=?2 AND recovery_activity_id IS NULL",
                params![
                    activity_id.into_bytes().as_slice(),
                    row.intent_id.into_bytes().as_slice()
                ],
            )
            .map_err(|error| StorageError::sqlite("link legacy recovery activity", error))?;
    }
    Ok(())
}

struct LegacyRow {
    intent_id: EffectIntentId,
    authorizing_activity_id: ActivityId,
    correlation_id: ActivityCorrelationId,
    episode_id: Option<EpisodeId>,
}

fn legacy_rows(transaction: &rusqlite::Transaction<'_>) -> Result<Vec<LegacyRow>, StorageError> {
    let mut statement = transaction
        .prepare(
            "SELECT recovery.intent_id,intent.authorizing_activity_id,intent.correlation_id,
         intent.episode_id FROM pod0_legacy_effect_recovery_v40 recovery
         JOIN pod0_effect_intents intent ON intent.intent_id=recovery.intent_id
         WHERE recovery.recovery_activity_id IS NULL ORDER BY recovery.intent_id",
        )
        .map_err(|error| StorageError::sqlite("read legacy effect recoveries", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Option<Vec<u8>>>(3)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("query legacy effect recoveries", error))?;
    rows.map(|row| {
        decode(row.map_err(|error| StorageError::sqlite("decode legacy effect recovery", error))?)
    })
    .collect()
}

fn decode(row: (Vec<u8>, Vec<u8>, Vec<u8>, Option<Vec<u8>>)) -> Result<LegacyRow, StorageError> {
    Ok(LegacyRow {
        intent_id: EffectIntentId::from_bytes(array(row.0)?),
        authorizing_activity_id: ActivityId::from_bytes(array(row.1)?),
        correlation_id: ActivityCorrelationId::from_bytes(array(row.2)?),
        episode_id: row.3.map(array).transpose()?.map(EpisodeId::from_bytes),
    })
}

fn array(value: Vec<u8>) -> Result<[u8; 16], StorageError> {
    value.try_into().map_err(|_| StorageError::InvalidActivity)
}

fn identities(intent_id: EffectIntentId) -> (ActivityId, ActivityTransactionId) {
    let mut hash = Sha256::new();
    hash.update(b"pod0/legacy-effect-recovery/v40");
    hash.update(intent_id.into_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    (
        ActivityId::from_bytes(digest[..16].try_into().unwrap()),
        ActivityTransactionId::from_bytes(digest[16..].try_into().unwrap()),
    )
}
