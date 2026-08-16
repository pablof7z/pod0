use pod0_application::{
    CancellationObservationActivityInput, DurableEffectExecution,
    DurableHostCancellationEffectRequest, DurableHostCancellationObservation,
    DurableHostCancellationOutcome, EffectOutcome, plan_cancellation_observation,
};
use pod0_domain::{CommandId, ContentDigest, EffectAttemptId, HostRequestId};
use rusqlite::{OptionalExtension, Transaction, params};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{LibraryStore, StorageError, TransitionIngress, TransitionIngressKind};

#[derive(Clone, Copy, Debug)]
pub struct CancellationObservationCommitInput {
    pub lease: pod0_application::PersistedEffectLeaseIdentity,
    pub observation: DurableHostCancellationObservation,
    pub committed_at: pod0_domain::UnixTimestampMilliseconds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CancellationObservationCommitOutcome {
    pub request_id: HostRequestId,
    pub replayed: bool,
}

impl LibraryStore {
    pub fn commit_cancellation_observation(
        &self,
        input: CancellationObservationCommitInput,
    ) -> Result<CancellationObservationCommitOutcome, StorageError> {
        let observed = input.observation;
        let identity = observation_identity(input.lease.attempt_id, observed.sequence_number);
        let fingerprint = fingerprint(&observed)?;
        let mut committed_request = None;
        let receipt = TransitionCommit::open(self.path())?.commit_planned_with_transaction_hooks(
            TransitionIngress {
                kind: TransitionIngressKind::HostObservation,
                id: identity.into_bytes(),
                fingerprint,
            },
            input.committed_at,
            |transaction| {
                let (effect, subject, episode_id) =
                    cancellation_request(transaction, input.lease.intent_id)?;
                validate_observation(&effect, &observed)?;
                committed_request = Some(effect);
                plan_cancellation_observation(CancellationObservationActivityInput {
                    attempt_id: input.lease.attempt_id,
                    intent_id: input.lease.intent_id,
                    authorizing_activity_id: input.lease.authorizing_activity_id,
                    correlation_id: input.lease.correlation_id,
                    subject,
                    episode_id,
                    request: effect,
                    outcome: observation_outcome(observed.outcome),
                })
                .map_err(|_| StorageError::InvalidActivity)
            },
            |transaction| stage_observation(transaction, input.lease, &observed),
            |_, expected, ()| Ok(expected),
            |transaction| {
                crate::effect_outbox::complete_host_observation_in_transaction(
                    transaction,
                    input.lease,
                )
                .map_err(|_| StorageError::CommandConflict)
            },
        )?;
        Ok(CancellationObservationCommitOutcome {
            request_id: committed_request.map_or(observed.request_id, |request| request.request_id),
            replayed: receipt.replayed,
        })
    }
}

fn cancellation_request(
    transaction: &Transaction<'_>,
    intent_id: pod0_domain::EffectIntentId,
) -> Result<
    (
        DurableHostCancellationEffectRequest,
        pod0_application::ActivitySubject,
        Option<pod0_domain::EpisodeId>,
    ),
    StorageError,
> {
    let json: String = transaction
        .query_row(
            "SELECT request_json FROM pod0_effect_intents WHERE intent_id=?1 AND effect_kind_code=16",
            [intent_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read cancellation effect", error))?;
    let durable: pod0_application::DurableExternalEffectRequest =
        serde_json::from_str(&json).map_err(|_| StorageError::InvalidActivity)?;
    let DurableEffectExecution::Cancellation { request } = durable.execution else {
        return Err(StorageError::InvalidActivity);
    };
    Ok((request, durable.subject, durable.episode_id))
}

fn validate_observation(
    request: &DurableHostCancellationEffectRequest,
    observed: &DurableHostCancellationObservation,
) -> Result<(), StorageError> {
    let exact = request.request_id == observed.request_id
        && request.cancellation_id == observed.cancellation_id
        && request.issued_revision == observed.observed_request_revision
        && match observed.outcome {
            DurableHostCancellationOutcome::Applied { target_request_id } => {
                target_request_id == request.target_request_id
            }
            DurableHostCancellationOutcome::Failed { .. } => true,
        };
    exact.then_some(()).ok_or(StorageError::CommandConflict)
}

fn stage_observation(
    transaction: &Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    observation: &DurableHostCancellationObservation,
) -> Result<(), StorageError> {
    let fence = i64::try_from(lease.fence).map_err(|_| StorageError::InvalidActivity)?;
    let observation_json =
        serde_json::to_string(observation).map_err(|_| StorageError::InvalidActivity)?;
    let outcome_json = serde_json::to_string(&observation_outcome(observation.outcome))
        .map_err(|_| StorageError::InvalidActivity)?;
    let row: Option<(i64, Option<String>)> = transaction
        .query_row(
            "SELECT a.state_code,a.observation_json FROM pod0_effect_attempts a \
             JOIN pod0_effect_intents i ON i.intent_id=a.intent_id \
             WHERE a.lease_id=?1 AND a.fence=?2 AND a.attempt_id=?3 AND a.intent_id=?4 \
             AND i.authorizing_activity_id=?5 AND i.correlation_id=?6 \
             AND a.lease_expires_at_ms>=?7 AND a.lease_expires_at_ms=?8 \
             AND i.effect_kind_code=16",
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
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("validate cancellation lease", error))?;
    let Some((state, stored)) = row else {
        return Err(StorageError::CommandConflict);
    };
    if state == 2 && stored.as_deref() == Some(&observation_json) {
        return Ok(());
    }
    if state != 1 {
        return Err(StorageError::CommandConflict);
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
        .map_err(|error| StorageError::sqlite("stage cancellation observation", error))?;
    (changed == 1)
        .then_some(())
        .ok_or(StorageError::CommandConflict)
}

fn observation_outcome(value: DurableHostCancellationOutcome) -> EffectOutcome {
    match value {
        DurableHostCancellationOutcome::Applied { .. } => EffectOutcome::Succeeded,
        DurableHostCancellationOutcome::Failed { code } => EffectOutcome::Failed {
            code: failure_code(code),
        },
    }
}

fn failure_code(code: pod0_application::HostFailureCode) -> pod0_application::ActivityFailureCode {
    use pod0_application::{ActivityFailureCode as Activity, HostFailureCode as Host};
    match code {
        Host::Offline => Activity::Offline,
        Host::TimedOut => Activity::TimedOut,
        Host::PermissionDenied => Activity::PermissionDenied,
        Host::InvalidResponse => Activity::InvalidResponse,
        Host::ResponseTooLarge => Activity::ResponseTooLarge,
        Host::MediaUnavailable => Activity::MediaUnavailable,
        Host::ProviderUnavailable => Activity::ProviderUnavailable,
        Host::Unauthorized => Activity::Unauthorized,
        Host::IndexUnavailable | Host::PlatformFailure => Activity::PlatformFailure,
        Host::Unsupported { wire_code } => Activity::Unsupported { wire_code },
    }
}

fn observation_identity(attempt: EffectAttemptId, sequence: u64) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-cancellation-observation-v1\0");
    hash.update(attempt.into_bytes());
    hash.update(sequence.to_be_bytes());
    CommandId::from_bytes(hash.finalize()[..16].try_into().expect("digest"))
}

fn fingerprint(value: &DurableHostCancellationObservation) -> Result<ContentDigest, StorageError> {
    let encoded = serde_json::to_vec(value).map_err(|_| StorageError::InvalidActivity)?;
    Ok(ContentDigest::from_bytes(Sha256::digest(encoded).into()))
}
