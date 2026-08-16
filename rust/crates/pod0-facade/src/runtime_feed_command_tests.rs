//! Lifecycle tests for the commit-immediately feed command family:
//! deterministic kernel-clock deadlines, drain safety, and cancellation
//! semantics against the durable workflow.

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[derive(Clone, Copy)]
struct FixedClock(i64);

impl pod0_application::Clock for FixedClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0)
    }
}

fn command(command_id: u64, cancellation_id: u64, payload: ApplicationCommand) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(0, command_id),
        cancellation_id: CancellationId::from_parts(0, cancellation_id),
        expected_revision: None,
        command: payload,
    }
}

fn library_request() -> ProjectionRequest {
    ProjectionRequest {
        scope: ProjectionScope::Library,
        offset: 0,
        max_items: 20,
    }
}

#[test]
fn command_deadlines_are_deterministic_from_the_injected_kernel_clock() {
    let first = PlaybackFixture::new();
    let second = PlaybackFixture::new();
    first
        .facade
        .state()
        .set_clock(std::sync::Arc::new(FixedClock(1_800_000_000_000)));
    second
        .facade
        .state()
        .set_clock(std::sync::Arc::new(FixedClock(1_800_000_000_000)));
    let envelope = command(
        1,
        10,
        ApplicationCommand::SubscribeToFeed {
            feed_url: "https://example.test/feed".to_owned(),
        },
    );

    first.facade.dispatch(envelope.clone());
    second.facade.dispatch(envelope);

    let first_request = subscribe_fetch_request(&first.facade);
    let second_request = subscribe_fetch_request(&second.facade);
    assert_eq!(first_request, second_request);
    assert_eq!(
        first_request.request.deadline_at,
        Some(UnixTimestampMilliseconds::new(
            1_800_000_000_000 + 24 * 60 * 60 * 1_000
        ))
    );
}

fn subscribe_fetch_request(facade: &Pod0Facade) -> LeasedHostRequestEnvelope {
    facade
        .next_leased_host_requests(20)
        .into_iter()
        .find(|request| {
            matches!(
                &request.request.request,
                HostRequest::FetchFeed { feed_url, .. }
                    if feed_url == "https://example.test/feed"
            )
        })
        .expect("subscribe command should issue one bounded host request")
}

#[test]
fn cancellation_prevents_late_host_observation_from_committing() {
    let fixture = PlaybackFixture::new();
    let facade = &fixture.facade;
    facade.dispatch(command(
        1,
        10,
        ApplicationCommand::SubscribeToFeed {
            feed_url: "https://example.test/feed".to_owned(),
        },
    ));
    let request = subscribe_fetch_request(facade);

    facade.dispatch(command(
        2,
        20,
        ApplicationCommand::CancelOperation {
            cancellation_id: CancellationId::from_parts(0, 10),
        },
    ));
    let cancellation = facade
        .next_leased_host_requests(20)
        .into_iter()
        .find(|leased| {
            matches!(
                leased.request.request,
                HostRequest::CancelAuthorizedEffect { target_request_id }
                    if target_request_id == request.request.request_id
            )
        })
        .expect("cancellation must be delivered only as a persisted exact lease");
    let cancellation_receipt = facade.record_leased_host_observation(
        LeasedHostObservationEnvelope {
            lease: cancellation.lease,
            observation: HostObservationEnvelope {
                request_id: cancellation.request.request_id,
                cancellation_id: cancellation.request.cancellation_id,
                observed_request_revision: cancellation.request.issued_revision,
                sequence_number: 0,
                observed_at: UnixTimestampMilliseconds::new(
                    cancellation.lease.expires_at.value - 1,
                ),
                observation: HostObservation::AuthorizedEffectCancellationApplied {
                    target_request_id: request.request.request_id,
                },
            },
        },
    );
    assert!(matches!(
        cancellation_receipt,
        HostObservationReceipt::Persisted { terminal: true, .. }
    ));
    let revision_after_cancel = facade.snapshot(library_request()).state_revision;

    let receipt = facade.record_leased_host_observation(LeasedHostObservationEnvelope {
        lease: request.lease,
        observation: HostObservationEnvelope {
            request_id: request.request.request_id,
            cancellation_id: request.request.cancellation_id,
            observed_request_revision: request.request.issued_revision,
            sequence_number: 0,
            observed_at: UnixTimestampMilliseconds::new(1_800_000_100_000),
            observation: HostObservation::FeedBytesFetched {
                bytes: br#"<rss version="2.0"><channel><title>Late</title>
<item><title>Late episode</title><guid>late-cancelled-episode</guid>
<enclosure url="https://example.test/late.mp3" type="audio/mpeg"/></item>
</channel></rss>"#
                    .to_vec(),
                entity_tag: None,
                last_modified: None,
                response_url: "https://example.test/feed".to_owned(),
                http_status: 200,
            },
        },
    });

    assert_eq!(
        receipt,
        HostObservationReceipt::Rejected {
            request_id: request.request.request_id,
            reason: HostObservationRejection::StaleWorkflow
        }
    );
    let snapshot = facade.snapshot(library_request());
    assert_eq!(
        snapshot.state_revision, revision_after_cancel,
        "a rejected late observation must leave state_revision unchanged"
    );
    let Projection::Library { value } = snapshot.projection else {
        panic!("expected library projection");
    };
    assert!(
        value
            .episodes
            .iter()
            .all(|episode| episode.publisher_guid != "late-cancelled-episode"),
        "a cancelled fetch must not commit its late result"
    );
}

#[test]
fn host_request_drain_is_safe_when_limit_exceeds_queue_length() {
    let fixture = PlaybackFixture::new();
    fixture.facade.dispatch(command(
        1,
        10,
        ApplicationCommand::SubscribeToFeed {
            feed_url: "https://example.test/feed".to_owned(),
        },
    ));

    let drained = fixture.facade.next_leased_host_requests(u16::MAX);
    let matching_fetches = drained
        .iter()
        .filter(|request| {
            matches!(
                &request.request.request,
                HostRequest::FetchFeed { feed_url, .. }
                    if feed_url == "https://example.test/feed"
            )
        })
        .count();
    assert_eq!(
        matching_fetches, 1,
        "subscribe must issue exactly one FetchFeed for the requested URL"
    );
    assert!(
        fixture
            .facade
            .next_leased_host_requests(u16::MAX)
            .is_empty()
    );
}

#[test]
fn cancellation_removes_native_work_that_has_not_started() {
    let fixture = PlaybackFixture::new();
    fixture.facade.dispatch(command(
        1,
        10,
        ApplicationCommand::SubscribeToFeed {
            feed_url: "https://example.test/feed".to_owned(),
        },
    ));
    fixture.facade.dispatch(command(
        2,
        20,
        ApplicationCommand::CancelOperation {
            cancellation_id: CancellationId::from_parts(0, 10),
        },
    ));

    assert!(
        fixture
            .facade
            .next_leased_host_requests(u16::MAX)
            .iter()
            .all(|request| !matches!(&request.request.request, HostRequest::FetchFeed { .. }))
    );
}
