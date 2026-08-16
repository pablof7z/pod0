use pod0_application::{
    DurableEffectExecution, DurableLifecycleEffectRequest, DurableLifecycleHostObservation,
    EffectOutcome, LifecycleWakeObservationInput, LifecycleWakeOutcome,
    plan_lifecycle_wake_observation,
};
use pod0_domain::{CommandId, ContentDigest, EffectAttemptId, HostRequestId};
use rusqlite::{OptionalExtension, Transaction, params};
use sha2::{Digest as _, Sha256};

use crate::transition_commit::TransitionCommit;
use crate::{LibraryStore, StorageError, TransitionIngress, TransitionIngressKind};

const MAX_WAKE_ATTEMPTS: u8 = 5;
const WAKE_RETRY_MILLISECONDS: i64 = 1_000;

#[derive(Clone, Debug)]
pub struct LifecycleWakeObservationCommitInput {
    pub lease: pod0_application::PersistedEffectLeaseIdentity,
    pub observation: DurableLifecycleHostObservation,
    pub committed_at: pod0_domain::UnixTimestampMilliseconds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LifecycleWakeObservationCommitOutcome {
    pub request_id: HostRequestId,
    pub reason: pod0_application::CoreWakeReason,
    pub reached: bool,
    pub replayed: bool,
}

impl LibraryStore {
    pub fn commit_lifecycle_wake_observation(
        &self,
        input: LifecycleWakeObservationCommitInput,
    ) -> Result<LifecycleWakeObservationCommitOutcome, StorageError> {
        let observed = input.observation.clone();
        let identity = observation_identity(input.lease.attempt_id, observed.sequence_number);
        let fingerprint = observation_fingerprint(&observed)?;
        let mut committed_request = None;
        let receipt = TransitionCommit::open(self.path())?.commit_planned_with_transaction_hooks(
            TransitionIngress {
                kind: TransitionIngressKind::HostObservation,
                id: identity.into_bytes(),
                fingerprint,
            },
            input.committed_at,
            |transaction| {
                let request = lifecycle_request(transaction, input.lease.intent_id)?;
                let outcome = effect_outcome(observed.outcome);
                let retry = retry_request(&request, observed.outcome, observed.observed_at.value);
                committed_request = Some(request.clone());
                plan_lifecycle_wake_observation(LifecycleWakeObservationInput {
                    identity_attempt_id: identity,
                    effect_attempt_id: input.lease.attempt_id,
                    intent_id: input.lease.intent_id,
                    authorizing_activity_id: input.lease.authorizing_activity_id,
                    correlation_id: input.lease.correlation_id,
                    subject: crate::transition_commit_lifecycle_wake::wake_subject(request.reason),
                    request,
                    outcome,
                    retry,
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
        let request = committed_request.unwrap_or_else(|| DurableLifecycleEffectRequest {
            request_id: observed.request_id,
            command_id: CommandId::from_bytes(observed.request_id.into_bytes()),
            cancellation_id: observed.cancellation_id,
            issued_revision: observed.observed_request_revision,
            wake_at: observed.observed_at,
            reason: match observed.outcome {
                LifecycleWakeOutcome::Reached { reason } => reason,
                _ => pod0_application::CoreWakeReason::Unsupported { wire_code: 0 },
            },
            attempt: 1,
        });
        Ok(LifecycleWakeObservationCommitOutcome {
            request_id: request.request_id,
            reason: request.reason,
            reached: matches!(
                observed.outcome,
                LifecycleWakeOutcome::Reached { reason } if reason == request.reason
            ),
            replayed: receipt.replayed,
        })
    }
}

fn lifecycle_request(
    transaction: &Transaction<'_>,
    intent_id: pod0_domain::EffectIntentId,
) -> Result<DurableLifecycleEffectRequest, StorageError> {
    let json: String = transaction
        .query_row(
            "SELECT request_json FROM pod0_effect_intents WHERE intent_id=?1 AND effect_kind_code=12",
            [intent_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read lifecycle wake effect", error))?;
    let effect: pod0_application::DurableExternalEffectRequest =
        serde_json::from_str(&json).map_err(|_| StorageError::InvalidActivity)?;
    let DurableEffectExecution::Lifecycle { request } = effect.execution else {
        return Err(StorageError::InvalidActivity);
    };
    Ok(request)
}

fn retry_request(
    request: &DurableLifecycleEffectRequest,
    outcome: LifecycleWakeOutcome,
    observed_at_ms: i64,
) -> Option<DurableLifecycleEffectRequest> {
    let retryable = matches!(
        outcome,
        LifecycleWakeOutcome::Failed {
            code: pod0_application::HostFailureCode::Offline
                | pod0_application::HostFailureCode::TimedOut
                | pod0_application::HostFailureCode::PlatformFailure
        }
    );
    let attempt = request.attempt.checked_add(1)?;
    if !retryable || attempt > MAX_WAKE_ATTEMPTS {
        return None;
    }
    let wake_at = observed_at_ms
        .saturating_add(WAKE_RETRY_MILLISECONDS.saturating_mul(i64::from(attempt - 1)))
        .max(request.wake_at.value);
    Some(DurableLifecycleEffectRequest {
        request_id: retry_request_id(request.request_id, attempt),
        command_id: request.command_id,
        cancellation_id: request.cancellation_id,
        issued_revision: request.issued_revision,
        wake_at: pod0_domain::UnixTimestampMilliseconds::new(wake_at),
        reason: request.reason,
        attempt,
    })
}

fn retry_request_id(request_id: HostRequestId, attempt: u8) -> HostRequestId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-lifecycle-wake-retry-v1\0");
    hash.update(request_id.into_bytes());
    hash.update([attempt]);
    HostRequestId::from_bytes(hash.finalize()[..16].try_into().expect("digest"))
}

fn effect_outcome(outcome: LifecycleWakeOutcome) -> EffectOutcome {
    match outcome {
        LifecycleWakeOutcome::Reached { .. } => EffectOutcome::Succeeded,
        LifecycleWakeOutcome::Cancelled => EffectOutcome::Cancelled,
        LifecycleWakeOutcome::Failed { code } => EffectOutcome::Failed {
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

include!("transition_commit_lifecycle_observation_stage.rs");
