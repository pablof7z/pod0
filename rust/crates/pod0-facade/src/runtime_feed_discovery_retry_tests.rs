use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use pod0_application::CoreWakeReason;

use crate::runtime_feed_persistence_tests::{
    FEED_WITH_NEW_EPISODE, configure_notifications_without_downloads, record_feed,
};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[derive(Clone)]
struct MutableClock(Arc<AtomicI64>);

impl pod0_application::Clock for MutableClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0.load(Ordering::SeqCst))
    }
}

#[test]
fn failed_notification_recovers_through_one_typed_wake_and_new_attempt() {
    let fixture = PlaybackFixture::new();
    let time = Arc::new(AtomicI64::new(1_800_000_100_000));
    fixture
        .facade
        .state()
        .set_clock(Arc::new(MutableClock(Arc::clone(&time))));
    configure_notifications_without_downloads(&fixture);
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(88, 1),
        cancellation_id: CancellationId::from_parts(89, 1),
        expected_revision: None,
        command: ApplicationCommand::RefreshPodcast {
            podcast_id: fixture.podcast_id,
        },
    });
    record_feed(&fixture, FEED_WITH_NEW_EPISODE);

    let first = fixture
        .facade
        .next_host_requests(10)
        .into_iter()
        .find(|request| {
            matches!(
                request.request,
                HostRequest::DeliverNewEpisodeNotification { .. }
            )
        })
        .expect("first notification request");
    let HostRequest::DeliverNewEpisodeNotification {
        occurrence_id,
        episode_id,
        ..
    } = first.request
    else {
        unreachable!()
    };
    assert_eq!(
        fixture
            .facade
            .record_host_observation(HostObservationEnvelope {
                request_id: first.request_id,
                cancellation_id: first.cancellation_id,
                observed_request_revision: first.issued_revision,
                sequence_number: 0,
                observed_at: UnixTimestampMilliseconds::new(time.load(Ordering::SeqCst)),
                observation: HostObservation::Failed {
                    code: HostFailureCode::PlatformFailure,
                    safe_detail: None,
                },
            }),
        HostObservationReceipt::Persisted {
            request_id: first.request_id,
            terminal: true,
        }
    );

    let wake = fixture
        .facade
        .next_host_requests(10)
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::ScheduleCoreWake { .. }))
        .expect("notification retry wake");
    let HostRequest::ScheduleCoreWake { wake_at, reason } = wake.request else {
        unreachable!()
    };
    assert!(matches!(
        reason,
        CoreWakeReason::FeedDiscoveryNotificationRetry {
            occurrence_id: expected_occurrence,
            episode_id: expected_episode,
            attempt: 1,
        } if expected_occurrence == occurrence_id && expected_episode == episode_id
    ));

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    reopened
        .state()
        .set_clock(Arc::new(MutableClock(Arc::clone(&time))));
    let recovered_wake = reopened
        .next_host_requests(10)
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::ScheduleCoreWake { .. }))
        .expect("recovered retry wake");
    assert_eq!(recovered_wake, wake);

    time.store(wake_at.value, Ordering::SeqCst);
    assert_eq!(
        reopened.record_host_observation(HostObservationEnvelope {
            request_id: recovered_wake.request_id,
            cancellation_id: recovered_wake.cancellation_id,
            observed_request_revision: recovered_wake.issued_revision,
            sequence_number: 0,
            observed_at: wake_at,
            observation: HostObservation::CoreWakeReached { reason },
        }),
        HostObservationReceipt::AcceptedTransient {
            request_id: recovered_wake.request_id,
        }
    );
    let second = reopened
        .next_host_requests(10)
        .into_iter()
        .find(|request| {
            matches!(
                request.request,
                HostRequest::DeliverNewEpisodeNotification { .. }
            )
        })
        .expect("second notification attempt");
    assert_ne!(second.request_id, first.request_id);
    assert!(matches!(
        second.request,
        HostRequest::DeliverNewEpisodeNotification {
            occurrence_id: expected_occurrence,
            episode_id: expected_episode,
            ..
        } if expected_occurrence == occurrence_id && expected_episode == episode_id
    ));
}
