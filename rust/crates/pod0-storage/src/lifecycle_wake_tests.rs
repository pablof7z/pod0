use pod0_application::{
    CoreWakeReason, DurableEffectExecution, DurableLifecycleEffectRequest,
    DurableLifecycleHostObservation, HostFailureCode, LifecycleWakeOutcome,
};
use pod0_domain::{
    CancellationId, CommandId, EpisodeId, HostRequestId, StateRevision, UnixTimestampMilliseconds,
};

use crate::feed_discovery_store_test_support::{BASE_TIME, empty_authoritative_store};
use crate::{LifecycleWakeObservationCommitInput, StorageError};

fn wake() -> DurableLifecycleEffectRequest {
    DurableLifecycleEffectRequest {
        request_id: HostRequestId::from_parts(31, 1),
        command_id: CommandId::from_parts(32, 1),
        cancellation_id: CancellationId::from_parts(33, 1),
        issued_revision: StateRevision::new(4),
        wake_at: UnixTimestampMilliseconds::new(BASE_TIME + 100),
        reason: CoreWakeReason::TranscriptRetry {
            episode_id: EpisodeId::from_parts(34, 1),
            attempt_id: pod0_domain::TranscriptAttemptId::from_parts(35, 1),
            submission_fence_id: pod0_domain::TranscriptSubmissionFenceId::from_parts(36, 1),
        },
        attempt: 1,
    }
}

#[test]
fn exact_wake_lease_survives_reopen_and_observation_replays_once() {
    let (fixture, store) = empty_authoritative_store();
    let request = wake();
    store
        .authorize_lifecycle_wake(request.clone(), UnixTimestampMilliseconds::new(BASE_TIME))
        .unwrap();
    drop(store);

    let reopened = crate::LibraryStore::open_authoritative(&fixture.target).unwrap();
    let lease = reopened
        .claim_next_effect(UnixTimestampMilliseconds::new(BASE_TIME), 10_000)
        .unwrap()
        .unwrap();
    assert!(matches!(
        &lease.request.execution,
        DurableEffectExecution::Lifecycle { request: exact } if exact == &request
    ));
    let observation = DurableLifecycleHostObservation {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: 1,
        observed_at: request.wake_at,
        outcome: LifecycleWakeOutcome::Reached {
            reason: request.reason,
        },
    };
    let input = LifecycleWakeObservationCommitInput {
        lease: lease.identity(),
        observation: observation.clone(),
        committed_at: request.wake_at,
    };
    let first = reopened
        .commit_lifecycle_wake_observation(input.clone())
        .unwrap();
    assert!(first.reached);
    assert!(!first.replayed);
    assert!(
        reopened
            .commit_lifecycle_wake_observation(input)
            .unwrap()
            .replayed
    );
    let stale = LifecycleWakeObservationCommitInput {
        lease: lease.identity(),
        observation: DurableLifecycleHostObservation {
            sequence_number: 2,
            ..observation
        },
        committed_at: request.wake_at,
    };
    assert_eq!(
        reopened.commit_lifecycle_wake_observation(stale),
        Err(StorageError::CommandConflict)
    );
}

#[test]
fn retryable_schedule_failure_atomically_authorizes_delayed_next_attempt() {
    let (_fixture, store) = empty_authoritative_store();
    let request = wake();
    store
        .authorize_lifecycle_wake(request.clone(), UnixTimestampMilliseconds::new(BASE_TIME))
        .unwrap();
    let lease = store
        .claim_next_effect(UnixTimestampMilliseconds::new(BASE_TIME), 10_000)
        .unwrap()
        .unwrap();
    store
        .commit_lifecycle_wake_observation(LifecycleWakeObservationCommitInput {
            lease: lease.identity(),
            observation: DurableLifecycleHostObservation {
                request_id: request.request_id,
                cancellation_id: request.cancellation_id,
                observed_request_revision: request.issued_revision,
                sequence_number: 1,
                observed_at: UnixTimestampMilliseconds::new(BASE_TIME + 10),
                outcome: LifecycleWakeOutcome::Failed {
                    code: HostFailureCode::PlatformFailure,
                },
            },
            committed_at: UnixTimestampMilliseconds::new(BASE_TIME + 10),
        })
        .unwrap();
    assert!(
        store
            .claim_next_effect(UnixTimestampMilliseconds::new(BASE_TIME + 1_009), 10_000)
            .unwrap()
            .is_none()
    );
    let retry = store
        .claim_next_effect(UnixTimestampMilliseconds::new(BASE_TIME + 1_010), 10_000)
        .unwrap()
        .unwrap();
    let DurableEffectExecution::Lifecycle { request: retry } = retry.request.execution else {
        panic!("expected lifecycle retry")
    };
    assert_eq!(retry.attempt, 2);
    assert_eq!(retry.reason, request.reason);
    assert_ne!(retry.request_id, request.request_id);
}

#[test]
fn lifecycle_cancellation_supersedes_unclaimed_wake_durably() {
    let (fixture, store) = empty_authoritative_store();
    let request = wake();
    store
        .authorize_lifecycle_wake(request.clone(), UnixTimestampMilliseconds::new(BASE_TIME))
        .unwrap();
    store
        .cancel_durable_lifecycle_wakes(
            CommandId::from_parts(40, 1),
            pod0_domain::ContentDigest::from_bytes([41; 32]),
            request.cancellation_id,
            UnixTimestampMilliseconds::new(BASE_TIME + 1),
        )
        .unwrap();
    drop(store);

    let reopened = crate::LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert!(
        reopened
            .claim_next_effect(UnixTimestampMilliseconds::new(BASE_TIME + 2), 10_000)
            .unwrap()
            .is_none()
    );
}
