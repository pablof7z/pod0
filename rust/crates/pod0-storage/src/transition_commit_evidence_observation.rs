use pod0_application::{EvidenceObservationActivityInput, plan_evidence_observation};
use pod0_domain::ContentDigest;
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{EffectOutboxError, StorageError, TransitionIngress, TransitionIngressKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceObservationCommitInput {
    pub lease: pod0_application::PersistedEffectLeaseIdentity,
    pub observation: pod0_application::DurableRecallHostObservation,
    pub committed_at: pod0_domain::UnixTimestampMilliseconds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvidenceObservationCommitOutcome {
    pub replayed: bool,
}

pub(crate) fn commit_evidence_observation(
    path: &std::path::Path,
    input: EvidenceObservationCommitInput,
) -> Result<EvidenceObservationCommitOutcome, StorageError> {
    let staged = input.observation.clone();
    let receipt = TransitionCommit::open(path)?.commit_planned_with_transaction_hooks(
        TransitionIngress {
            kind: TransitionIngressKind::HostObservation,
            id: input.lease.attempt_id.into_bytes(),
            fingerprint: fingerprint(&input),
        },
        input.committed_at,
        |transaction| {
            let current_revision = core_revision(transaction)?;
            plan_evidence_observation(EvidenceObservationActivityInput {
                request_id: input.observation.request_id,
                episode_id: input.observation.episode_id,
                generation_id: input.observation.generation_id,
                intent_id: input.lease.intent_id,
                attempt_id: input.lease.attempt_id,
                authorizing_activity_id: input.lease.authorizing_activity_id,
                correlation_id: input.lease.correlation_id,
                current_revision,
                observation: input.observation.clone(),
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction| {
            crate::effect_outbox::stage_recall_observation_in_transaction(
                transaction,
                input.lease,
                &staged,
            )
            .map_err(effect_error)
        },
        |transaction, expected, observation| {
            if observation.episode_id != input.observation.episode_id
                || observation.generation_id != input.observation.generation_id
            {
                return Err(StorageError::RevisionConflict);
            }
            require_core_revision(transaction, expected)?;
            crate::library_store::advance_playback_revision(transaction)
        },
        |transaction| {
            crate::effect_outbox::complete_host_observation_in_transaction(transaction, input.lease)
                .map_err(effect_error)
        },
    )?;
    Ok(EvidenceObservationCommitOutcome {
        replayed: receipt.replayed,
    })
}

fn core_revision(connection: &rusqlite::Connection) -> Result<pod0_domain::StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read evidence observation revision", error))?;
    super::application_support::revision(value)
}

fn require_core_revision(
    connection: &rusqlite::Connection,
    expected: pod0_domain::StateRevision,
) -> Result<(), StorageError> {
    (core_revision(connection)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}

fn fingerprint(input: &EvidenceObservationCommitInput) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/evidence/provider-observation/v1");
    hash.update(input.lease.intent_id.into_bytes());
    hash.update(input.lease.attempt_id.into_bytes());
    hash.update(serde_json::to_vec(&input.observation).expect("typed recall observation"));
    ContentDigest::from_bytes(hash.finalize().into())
}

fn effect_error(error: EffectOutboxError) -> StorageError {
    match error {
        EffectOutboxError::StaleLease => StorageError::StaleTranscriptAttempt,
        EffectOutboxError::InvalidRecord | EffectOutboxError::InvalidLeaseDuration => {
            StorageError::InvalidActivity
        }
        EffectOutboxError::Storage => StorageError::Sqlite {
            operation: "commit recall effect observation",
        },
    }
}
