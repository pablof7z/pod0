use std::sync::Mutex;

use crate::*;

#[derive(Clone, Copy)]
struct FixedClock(i64);

impl pod0_application::Clock for FixedClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0)
    }
}

#[derive(Default)]
struct RecordingSubscriber {
    projections: Mutex<Vec<ProjectionEnvelope>>,
}

impl RecordingSubscriber {
    fn count(&self) -> usize {
        self.projections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    fn last(&self) -> ProjectionEnvelope {
        self.projections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .last()
            .cloned()
            .expect("subscriber must have received a projection")
    }
}

impl ProjectionSubscriber for RecordingSubscriber {
    fn receive(&self, projection: ProjectionEnvelope) {
        self.projections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(projection);
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
    use crate::runtime_playback_test_support::PlaybackFixture;

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
        first_request.deadline_at,
        Some(UnixTimestampMilliseconds::new(
            1_800_000_000_000 + 24 * 60 * 60 * 1_000
        ))
    );
}

fn subscribe_fetch_request(facade: &Pod0Facade) -> HostRequestEnvelope {
    facade
        .next_host_requests(20)
        .into_iter()
        .find(|request| {
            matches!(
                &request.request,
                HostRequest::FetchFeed { feed_url, .. }
                    if feed_url == "https://example.test/feed"
            )
        })
        .expect("subscribe command should issue one bounded host request")
}

#[test]
fn subscription_is_event_driven_and_unsubscribe_stops_delivery() {
    let facade = Pod0Facade::new();
    let subscriber = std::sync::Arc::new(RecordingSubscriber::default());
    let handle = facade.subscribe(library_request(), subscriber.clone());
    assert_eq!(subscriber.count(), 1);

    facade.dispatch(command(
        1,
        10,
        ApplicationCommand::Unsupported { wire_code: 77 },
    ));
    assert_eq!(subscriber.count(), 2);
    assert!(!subscriber.last().content_changed);

    facade.unsubscribe(handle);
    facade.dispatch(command(
        2,
        20,
        ApplicationCommand::Unsupported { wire_code: 78 },
    ));
    assert_eq!(subscriber.count(), 2);
}

#[test]
fn subscription_does_not_redeliver_an_unchanged_projection() {
    let facade = Pod0Facade::new();
    let subscriber = std::sync::Arc::new(RecordingSubscriber::default());
    facade.subscribe(
        ProjectionRequest {
            scope: ProjectionScope::Playback,
            offset: 0,
            max_items: 1,
        },
        subscriber.clone(),
    );
    assert_eq!(subscriber.count(), 1);

    facade.dispatch(command(
        1,
        10,
        ApplicationCommand::Unsupported { wire_code: 77 },
    ));

    assert_eq!(subscriber.count(), 1);
}

#[test]
fn library_subscription_detects_changes_beyond_its_bounded_page() {
    use crate::runtime_playback_test_support::PlaybackFixture;

    let fixture = PlaybackFixture::new();
    {
        let mut state = fixture.facade.state();
        let template = state
            .listening
            .episodes
            .first()
            .cloned()
            .expect("fixture episode");
        for ordinal in 1..25_u64 {
            let mut episode = template.clone();
            episode.episode_id = EpisodeId::from_parts(91, ordinal);
            episode.publisher_guid = format!("page-two-{ordinal}");
            episode.title = format!("Page two episode {ordinal}");
            state.listening.episodes.push(episode);
        }
    }

    let subscriber = std::sync::Arc::new(RecordingSubscriber::default());
    fixture
        .facade
        .subscribe(library_request(), subscriber.clone());
    assert_eq!(subscriber.count(), 1);

    let was_starred = fixture.facade.state().listening.episodes[24].is_starred;
    fixture.facade.state().listening.episodes[24].is_starred = !was_starred;
    fixture.facade.notify_subscribers();

    assert_eq!(subscriber.count(), 2);
    assert!(subscriber.last().content_changed);
}

#[test]
fn cancellation_prevents_late_host_observation_from_committing() {
    use crate::runtime_playback_test_support::PlaybackFixture;

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

    let receipt = facade.record_host_observation(HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
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
    });

    assert_eq!(
        receipt,
        HostObservationReceipt::Rejected {
            request_id: request.request_id,
            reason: HostObservationRejection::Cancelled
        }
    );
    let Projection::Library { value } = facade.snapshot(library_request()).projection else {
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
    use crate::runtime_playback_test_support::PlaybackFixture;

    let fixture = PlaybackFixture::new();
    fixture.facade.dispatch(command(
        1,
        10,
        ApplicationCommand::SubscribeToFeed {
            feed_url: "https://example.test/feed".to_owned(),
        },
    ));

    let drained = fixture.facade.next_host_requests(u16::MAX);
    assert!(drained.iter().any(|request| {
        matches!(
            &request.request,
            HostRequest::FetchFeed { feed_url, .. }
                if feed_url == "https://example.test/feed"
        )
    }));
    assert!(fixture.facade.next_host_requests(u16::MAX).is_empty());
}

#[test]
fn cancellation_removes_native_work_that_has_not_started() {
    use crate::runtime_playback_test_support::PlaybackFixture;

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
            .next_host_requests(u16::MAX)
            .iter()
            .all(|request| !matches!(&request.request, HostRequest::FetchFeed { .. }))
    );
}

#[test]
fn revision_conflict_is_terminal_for_the_command_identity() {
    let facade = Pod0Facade::new();
    facade.dispatch(command(
        1,
        10,
        ApplicationCommand::Unsupported { wire_code: 1 },
    ));
    let stale = CommandEnvelope {
        command_id: CommandId::from_parts(0, 2),
        cancellation_id: CancellationId::from_parts(0, 20),
        expected_revision: Some(StateRevision::INITIAL),
        command: ApplicationCommand::Unsupported { wire_code: 2 },
    };
    facade.dispatch(stale.clone());
    let conflict_revision = facade.snapshot(library_request()).state_revision;

    facade.dispatch(stale);

    assert_eq!(
        facade.snapshot(library_request()).state_revision,
        conflict_revision
    );
}
