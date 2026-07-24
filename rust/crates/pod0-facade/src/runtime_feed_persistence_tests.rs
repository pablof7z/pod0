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

fn record_feed(fixture: &PlaybackFixture, feed: &str) {
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

const FEED_WITH_NEW_EPISODE: &str = r#"
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
