use std::sync::Arc;

use crate::runtime_feed_persistence_tests::{
    FEED_WITH_NEW_EPISODE, configure_notifications_without_downloads, record_feed,
};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[derive(Clone, Copy)]
struct FixedClock(i64);

impl pod0_application::Clock for FixedClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0)
    }
}

#[test]
fn failed_notification_retry_is_an_exact_delayed_effect_that_survives_restart() {
    let fixture = PlaybackFixture::new();
    let now_ms = 1_800_000_100_000;
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(now_ms)));
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
    let first = notification_request(&fixture.facade);
    let HostRequest::DeliverNewEpisodeNotification {
        occurrence_id,
        episode_id,
        ..
    } = first.request.request
    else {
        unreachable!()
    };
    let receipt = fixture
        .facade
        .record_leased_host_observation(LeasedHostObservationEnvelope {
            lease: first.lease,
            observation: HostObservationEnvelope {
                request_id: first.request.request_id,
                cancellation_id: first.request.cancellation_id,
                observed_request_revision: first.request.issued_revision,
                sequence_number: 0,
                observed_at: UnixTimestampMilliseconds::new(now_ms),
                observation: HostObservation::Failed {
                    code: HostFailureCode::PlatformFailure,
                    safe_detail: None,
                },
            },
        });
    assert!(matches!(receipt, HostObservationReceipt::Persisted { .. }));
    assert!(fixture.facade.next_leased_host_requests(10).is_empty());

    let reopened = Pod0Facade::open_with_clock(
        fixture.target.to_string_lossy().into_owned(),
        Arc::new(FixedClock(now_ms + 86_400_000)),
    );
    let second = notification_request(&reopened);
    assert_ne!(second.request.request_id, first.request.request_id);
    assert!(matches!(
        second.request.request,
        HostRequest::DeliverNewEpisodeNotification {
            occurrence_id: actual_occurrence,
            episode_id: actual_episode,
            ..
        } if actual_occurrence == occurrence_id && actual_episode == episode_id
    ));
}

fn notification_request(facade: &Pod0Facade) -> LeasedHostRequestEnvelope {
    facade
        .next_leased_host_requests(10)
        .into_iter()
        .find(|request| {
            matches!(
                request.request.request,
                HostRequest::DeliverNewEpisodeNotification { .. }
            )
        })
        .expect("notification effect must be leaseable")
}
