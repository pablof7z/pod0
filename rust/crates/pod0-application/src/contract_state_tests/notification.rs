use pod0_domain::{FeedDiscoveryOccurrenceId, PodcastId};

use super::*;

fn request() -> HostRequestEnvelope {
    HostRequestEnvelope {
        request_id: HostRequestId::from_parts(0, 101),
        command_id: CommandId::from_parts(0, 102),
        cancellation_id: CancellationId::from_parts(0, 103),
        issued_revision: StateRevision::new(104),
        deadline_at: Some(UnixTimestampMilliseconds::new(2_000)),
        request: HostRequest::DeliverNewEpisodeNotification {
            occurrence_id: FeedDiscoveryOccurrenceId::from_parts(0, 105),
            episode_id: EpisodeId::from_parts(0, 106),
            podcast_id: PodcastId::from_parts(0, 107),
            podcast_title: "Podcast".into(),
            episode_title: "Episode".into(),
        },
    }
}

fn observation(
    occurrence_id: FeedDiscoveryOccurrenceId,
    episode_id: EpisodeId,
) -> HostObservationEnvelope {
    HostObservationEnvelope {
        request_id: HostRequestId::from_parts(0, 101),
        cancellation_id: CancellationId::from_parts(0, 103),
        observed_request_revision: StateRevision::new(104),
        sequence_number: 0,
        observed_at: UnixTimestampMilliseconds::new(1_000),
        observation: HostObservation::NewEpisodeNotificationDelivered {
            occurrence_id,
            episode_id,
        },
    }
}

#[test]
fn notification_observation_requires_the_exact_occurrence_and_episode() {
    let mut ledger = HostRequestLedger::default();
    assert!(ledger.register(request()));

    assert_eq!(
        ledger.accept_observation(&observation(
            FeedDiscoveryOccurrenceId::from_parts(0, 999),
            EpisodeId::from_parts(0, 106),
        )),
        ObservationAcceptance::MismatchedPayload
    );
    assert_eq!(
        ledger.accept_observation(&observation(
            FeedDiscoveryOccurrenceId::from_parts(0, 105),
            EpisodeId::from_parts(0, 999),
        )),
        ObservationAcceptance::MismatchedPayload
    );

    let exact = observation(
        FeedDiscoveryOccurrenceId::from_parts(0, 105),
        EpisodeId::from_parts(0, 106),
    );
    assert_eq!(
        ledger.accept_observation(&exact),
        ObservationAcceptance::Accepted
    );
    assert_eq!(
        ledger.accept_observation(&exact),
        ObservationAcceptance::Duplicate
    );
}
