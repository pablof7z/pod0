//! Shared helpers and the durable feed fixture for the feed-fetch recovery
//! tests; everything here is projection plumbing, not assertions.

use crate::*;

pub(super) const DURABLE_FEED: &str = r#"
<rss version="2.0"><channel><title>Durable workflow fixture</title>
<item><title>Durable episode</title><guid>durable-workflow-episode</guid>
<pubDate>Mon, 20 Jul 2026 09:00:00 GMT</pubDate>
<enclosure url="https://durable.example/durable.mp3" type="audio/mpeg"/></item>
</channel></rss>"#;

#[derive(Clone, Copy)]
pub(super) struct FixedClock(pub(super) i64);

impl pod0_application::Clock for FixedClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0)
    }
}

pub(super) fn subscribe(facade: &Pod0Facade, id: u64, feed_url: &str) -> CommandId {
    let command_id = CommandId::from_parts(88, id);
    facade.dispatch(CommandEnvelope {
        command_id,
        cancellation_id: CancellationId::from_parts(89, id),
        expected_revision: None,
        command: ApplicationCommand::SubscribeToFeed {
            feed_url: feed_url.to_owned(),
        },
    });
    command_id
}

pub(super) fn library(facade: &Pod0Facade) -> LibraryProjection {
    let Projection::Library { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Library,
            offset: 0,
            max_items: 50,
        })
        .projection
    else {
        panic!("expected library projection");
    };
    value
}

pub(super) fn operation(facade: &Pod0Facade, command_id: CommandId) -> OperationProjection {
    library(facade)
        .operations
        .into_iter()
        .find(|operation| operation.command_id == command_id)
        .expect("operation should be projected")
}

pub(super) fn subscribed_podcast_id(facade: &Pod0Facade, feed_url: &str) -> Option<PodcastId> {
    let identity = pod0_application::normalize_feed_url(feed_url).unwrap();
    let value = library(facade);
    let podcast_id = value.podcasts.iter().find_map(|podcast| {
        podcast
            .feed_identity
            .as_ref()
            .filter(|feed| feed.comparison_key == identity.comparison_key)
            .map(|_| podcast.podcast_id)
    })?;
    value
        .subscriptions
        .iter()
        .find(|row| row.podcast_id == podcast_id)
        .map(|row| row.podcast_id)
}

pub(super) fn fetch_requests_for(
    requests: &[HostRequestEnvelope],
    feed_url: &str,
) -> Vec<HostRequestEnvelope> {
    let identity = pod0_application::normalize_feed_url(feed_url).unwrap();
    requests
        .iter()
        .filter(|request| {
            matches!(
                &request.request,
                HostRequest::FetchFeed { feed_url, .. } if *feed_url == identity.source_url
            )
        })
        .cloned()
        .collect()
}

pub(super) fn feed_bytes_observation(request: &HostRequestEnvelope) -> HostObservationEnvelope {
    let HostRequest::FetchFeed { feed_url, .. } = &request.request else {
        panic!("expected feed fetch request");
    };
    HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: 0,
        observed_at: UnixTimestampMilliseconds::new(1_800_000_100_000),
        observation: HostObservation::FeedBytesFetched {
            bytes: DURABLE_FEED.as_bytes().to_vec(),
            entity_tag: Some("\"durable-v1\"".to_owned()),
            last_modified: None,
            response_url: feed_url.clone(),
            http_status: 200,
        },
    }
}

pub(super) fn durable_episode_count(facade: &Pod0Facade) -> usize {
    library(facade)
        .episodes
        .iter()
        .filter(|episode| episode.publisher_guid == "durable-workflow-episode")
        .count()
}
