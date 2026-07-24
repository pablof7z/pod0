use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn only_refresh_intent_commits_feed_discovery_evidence() {
    let fixture = PlaybackFixture::new();
    let refresh = CommandEnvelope {
        command_id: CommandId::from_parts(80, 1),
        cancellation_id: CancellationId::from_parts(81, 1),
        expected_revision: None,
        command: ApplicationCommand::RefreshPodcast {
            podcast_id: fixture.podcast_id,
        },
    };
    fixture.facade.dispatch(refresh);
    record_feed(&fixture, FEED_WITH_NEW_EPISODE);

    let state = fixture.facade.state();
    let store = state.store.as_ref().unwrap();
    let pending = store.pending_feed_discoveries(10).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].command_id, CommandId::from_parts(80, 1));
    assert_eq!(pending[0].items.len(), 1);
    assert!(!pending[0].is_initial_population);
    drop(state);

    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(80, 2),
        cancellation_id: CancellationId::from_parts(81, 2),
        expected_revision: None,
        command: ApplicationCommand::HydratePodcastMetadata {
            podcast_id: fixture.podcast_id,
        },
    });
    record_feed(&fixture, FEED_WITH_METADATA_ONLY_EPISODE);

    let state = fixture.facade.state();
    let store = state.store.as_ref().unwrap();
    assert_eq!(store.pending_feed_discoveries(10).unwrap(), pending);
    assert!(
        state
            .listening
            .episodes
            .iter()
            .all(|episode| episode.publisher_guid != "metadata-only")
    );
}

#[test]
fn notification_request_recovers_with_exact_identity_and_commits_once() {
    let fixture = PlaybackFixture::new();
    configure_notifications_without_downloads(&fixture);
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(82, 1),
        cancellation_id: CancellationId::from_parts(83, 1),
        expected_revision: None,
        command: ApplicationCommand::RefreshPodcast {
            podcast_id: fixture.podcast_id,
        },
    });
    record_feed(&fixture, FEED_WITH_NEW_EPISODE);
    let request = fixture
        .facade
        .next_host_requests(10)
        .into_iter()
        .find(|request| {
            matches!(
                request.request,
                HostRequest::DeliverNewEpisodeNotification { .. }
            )
        })
        .unwrap();
    let HostRequest::DeliverNewEpisodeNotification {
        occurrence_id,
        episode_id,
        podcast_id,
        podcast_title,
        episode_title,
    } = request.request.clone()
    else {
        unreachable!()
    };
    assert_eq!(podcast_id, fixture.podcast_id);
    assert_eq!(podcast_title, "Legacy Kotlin fixture");
    assert_eq!(episode_title, "New episode");

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let recovered = reopened
        .next_host_requests(10)
        .into_iter()
        .find(|candidate| candidate.request_id == request.request_id)
        .unwrap();
    assert_eq!(recovered, request);
    let receipt = reopened.record_host_observation(HostObservationEnvelope {
        request_id: recovered.request_id,
        cancellation_id: recovered.cancellation_id,
        observed_request_revision: recovered.issued_revision,
        sequence_number: 0,
        observed_at: UnixTimestampMilliseconds::new(1_800_000_100_001),
        observation: HostObservation::NewEpisodeNotificationDelivered {
            occurrence_id,
            episode_id,
        },
    });
    assert_eq!(
        receipt,
        HostObservationReceipt::Persisted {
            request_id: recovered.request_id,
            terminal: true
        }
    );
    assert_eq!(
        reopened.record_host_observation(HostObservationEnvelope {
            request_id: recovered.request_id,
            cancellation_id: recovered.cancellation_id,
            observed_request_revision: recovered.issued_revision,
            sequence_number: 0,
            observed_at: UnixTimestampMilliseconds::new(1_800_000_100_001),
            observation: HostObservation::NewEpisodeNotificationDelivered {
                occurrence_id,
                episode_id,
            },
        }),
        HostObservationReceipt::Rejected {
            request_id: recovered.request_id,
            reason: HostObservationRejection::UnknownRequest
        }
    );
    let relaunched = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    assert!(relaunched.next_host_requests(10).into_iter().all(|request| {
        !matches!(
            request.request,
            HostRequest::DeliverNewEpisodeNotification { .. }
        )
    }));
}

#[test]
fn global_notification_setting_is_projected_and_withdraws_pending_delivery() {
    let fixture = PlaybackFixture::new();
    configure_notifications_without_downloads(&fixture);
    let Projection::NewEpisodeNotificationSettings { value } = fixture
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::NewEpisodeNotificationSettings,
            offset: 0,
            max_items: 1,
        })
        .projection
    else {
        panic!("expected notification settings projection");
    };
    assert!(value.enabled);

    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(84, 1),
        cancellation_id: CancellationId::from_parts(85, 1),
        expected_revision: None,
        command: ApplicationCommand::RefreshPodcast {
            podcast_id: fixture.podcast_id,
        },
    });
    record_feed(&fixture, FEED_WITH_NEW_EPISODE);
    let request = fixture
        .facade
        .next_host_requests(10)
        .into_iter()
        .find(|request| {
            matches!(
                request.request,
                HostRequest::DeliverNewEpisodeNotification { .. }
            )
        })
        .unwrap();
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(84, 2),
        cancellation_id: CancellationId::from_parts(85, 2),
        expected_revision: None,
        command: ApplicationCommand::SetNewEpisodeNotificationsEnabled { enabled: false },
    });
    assert_eq!(
        fixture.facade.next_host_cancellations(10),
        [HostCancellationRequest {
            request_id: request.request_id,
            cancellation_id: request.cancellation_id,
        }]
    );
    let Projection::NewEpisodeNotificationSettings { value } = fixture
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::NewEpisodeNotificationSettings,
            offset: 0,
            max_items: 1,
        })
        .projection
    else {
        panic!("expected notification settings projection");
    };
    assert!(!value.enabled);
}

pub(super) fn configure_notifications_without_downloads(fixture: &PlaybackFixture) {
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(86, 1),
        cancellation_id: CancellationId::from_parts(87, 1),
        expected_revision: None,
        command: ApplicationCommand::SetSubscriptionAutoDownload {
            podcast_id: fixture.podcast_id,
            policy: AutoDownloadPolicy {
                mode: AutoDownloadMode::Off,
                wifi_only: false,
            },
        },
    });
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(86, 2),
        cancellation_id: CancellationId::from_parts(87, 2),
        expected_revision: None,
        command: ApplicationCommand::SetSubscriptionNotifications {
            podcast_id: fixture.podcast_id,
            enabled: true,
        },
    });
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(86, 3),
        cancellation_id: CancellationId::from_parts(87, 3),
        expected_revision: None,
        command: ApplicationCommand::SetNewEpisodeNotificationsEnabled { enabled: true },
    });
}

pub(super) fn record_feed(fixture: &PlaybackFixture, feed: &str) {
    let request = fixture.facade.next_host_requests(1).pop().unwrap();
    assert!(matches!(request.request, HostRequest::FetchFeed { .. }));
    fixture
        .facade
        .record_host_observation(HostObservationEnvelope {
            request_id: request.request_id,
            cancellation_id: request.cancellation_id,
            observed_request_revision: request.issued_revision,
            sequence_number: 0,
            observed_at: UnixTimestampMilliseconds::new(1_800_000_100_000),
            observation: HostObservation::FeedBytesFetched {
                bytes: feed.as_bytes().to_vec(),
                entity_tag: Some("\"feed-v2\"".to_owned()),
                last_modified: None,
                response_url: "https://legacy.example/feed".to_owned(),
                http_status: 200,
            },
        });
}

pub(super) const FEED_WITH_NEW_EPISODE: &str = r#"
<rss version="2.0"><channel><title>Legacy Kotlin fixture</title>
<item><title>New episode</title><guid>new-feed-guid</guid>
<pubDate>Mon, 20 Jul 2026 09:00:00 GMT</pubDate>
<enclosure url="https://legacy.example/new.mp3" type="audio/mpeg"/></item>
</channel></rss>"#;

const FEED_WITH_METADATA_ONLY_EPISODE: &str = r#"
<rss version="2.0"><channel><title>Retitled fixture</title>
<item><title>Metadata-only episode</title><guid>metadata-only</guid>
<pubDate>Tue, 21 Jul 2026 09:00:00 GMT</pubDate>
<enclosure url="https://legacy.example/metadata-only.mp3" type="audio/mpeg"/></item>
</channel></rss>"#;
