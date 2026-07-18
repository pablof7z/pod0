use pod0_domain::{CancellationId, CommandId, EpisodeId, HostRequestId, StateRevision};

use crate::{
    ApplicationCommand, CommandEnvelope, CommandLedger, CommandRegistration, HostObservation,
    HostObservationEnvelope, HostRequest, HostRequestEnvelope, HostRequestLedger,
    ObservationAcceptance, ProjectionRequest, ProjectionScope, SubscriptionRegistry,
};

fn command(command_id: u64, expected: Option<u64>) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(0, command_id),
        cancellation_id: CancellationId::from_parts(0, command_id),
        expected_revision: expected.map(StateRevision::new),
        command: ApplicationCommand::RequestPlayback {
            episode_id: EpisodeId::from_parts(1, 1),
        },
    }
}

fn host_request() -> HostRequestEnvelope {
    HostRequestEnvelope {
        request_id: HostRequestId::from_parts(0, 7),
        command_id: CommandId::from_parts(0, 1),
        cancellation_id: CancellationId::from_parts(0, 2),
        issued_revision: StateRevision::new(4),
        request: HostRequest::StopPlayback {
            episode_id: EpisodeId::from_parts(1, 1),
        },
    }
}

fn observation() -> HostObservationEnvelope {
    HostObservationEnvelope {
        request_id: HostRequestId::from_parts(0, 7),
        cancellation_id: CancellationId::from_parts(0, 2),
        observed_request_revision: StateRevision::new(4),
        observation: HostObservation::Cancelled,
    }
}

#[test]
fn command_retries_are_idempotent_and_reuse_conflicts() {
    let mut ledger = CommandLedger::default();
    let first = command(1, Some(3));

    assert_eq!(
        ledger.register(first.clone(), StateRevision::new(3)),
        CommandRegistration::Accepted
    );
    assert_eq!(
        ledger.register(first, StateRevision::new(99)),
        CommandRegistration::Duplicate
    );
    assert_eq!(
        ledger.register(command(1, None), StateRevision::new(3)),
        CommandRegistration::ConflictingReuse
    );
    assert_eq!(
        ledger.register(command(2, Some(2)), StateRevision::new(3)),
        CommandRegistration::StaleRevision
    );
}

#[test]
fn observations_commit_once_and_late_cancelled_results_are_rejected() {
    let mut completed = HostRequestLedger::default();
    assert!(completed.register(host_request()));
    assert_eq!(
        completed.accept_observation(&observation()),
        ObservationAcceptance::Accepted
    );
    assert_eq!(
        completed.accept_observation(&observation()),
        ObservationAcceptance::Duplicate
    );

    let mut cancelled = HostRequestLedger::default();
    assert!(cancelled.register(host_request()));
    assert_eq!(cancelled.cancel(CancellationId::from_parts(0, 2)), 1);
    assert_eq!(
        cancelled.accept_observation(&observation()),
        ObservationAcceptance::Cancelled
    );
}

#[test]
fn mismatched_or_oversized_host_results_cannot_commit() {
    let mut ledger = HostRequestLedger::default();
    let mut request = host_request();
    request.request = HostRequest::FetchFeed {
        feed_url: "https://example.test/feed".to_owned(),
        maximum_response_bytes: 2,
    };
    assert!(ledger.register(request));

    let mut result = observation();
    result.cancellation_id = CancellationId::from_parts(0, 99);
    assert_eq!(
        ledger.accept_observation(&result),
        ObservationAcceptance::CancellationMismatch
    );

    result.cancellation_id = CancellationId::from_parts(0, 2);
    result.observed_request_revision = StateRevision::new(3);
    assert_eq!(
        ledger.accept_observation(&result),
        ObservationAcceptance::StaleRequestRevision
    );

    result.observed_request_revision = StateRevision::new(4);
    result.observation = HostObservation::FeedBytesFetched {
        bytes: vec![1, 2, 3],
        entity_tag: None,
        last_modified: None,
    };
    assert_eq!(
        ledger.accept_observation(&result),
        ObservationAcceptance::PayloadTooLarge
    );
}

#[test]
fn unsubscribe_is_explicit_and_handles_are_not_reused() {
    let mut registry = SubscriptionRegistry::default();
    let request = ProjectionRequest {
        scope: ProjectionScope::Library,
        max_items: 40,
    };
    let first = registry.subscribe(request);

    assert_eq!(registry.request(first), Some(request));
    assert!(registry.unsubscribe(first));
    assert!(!registry.unsubscribe(first));
    let second = registry.subscribe(request);
    assert_ne!(first, second);
}
