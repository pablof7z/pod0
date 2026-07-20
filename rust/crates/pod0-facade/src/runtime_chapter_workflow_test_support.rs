use std::sync::Arc;

use rusqlite::Connection;
use sha2::Digest as _;

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[derive(Clone, Copy)]
struct FixedClock(i64);

impl pod0_application::Clock for FixedClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0)
    }
}

pub(super) fn publisher_fixture() -> PlaybackFixture {
    let fixture = PlaybackFixture::new();
    set_source(&fixture, Some("https://example.test/chapters.json"));
    fixture
}

pub(super) fn empty_fixture() -> PlaybackFixture {
    PlaybackFixture::new()
}

pub(super) fn set_source(fixture: &PlaybackFixture, source: Option<&str>) {
    set_source_for(fixture, fixture.episode_id, source);
}

pub(super) fn set_source_for(
    fixture: &PlaybackFixture,
    episode_id: EpisodeId,
    source: Option<&str>,
) {
    let connection = Connection::open(&fixture.target).unwrap();
    let changed = connection
        .execute(
            "UPDATE pod0_episode_feed_metadata SET chapters_url=?1 WHERE episode_id=?2",
            rusqlite::params![source, episode_id.into_bytes().as_slice()],
        )
        .unwrap();
    assert_eq!(changed, 1);
}

pub(super) fn publisher_artifact(
    fixture: &PlaybackFixture,
    document: Vec<u8>,
) -> ChapterArtifactInput {
    let projection = qualify_publisher_chapter_observation(PublisherChapterObservation {
        episode_id: fixture.episode_id,
        podcast_id: fixture.podcast_id,
        resolved_source_url: "https://example.test/selected.json".to_owned(),
        content_type: "application/json".to_owned(),
        payload_digest: ContentDigest::from_bytes(sha2::Sha256::digest(&document).into()),
        payload: document,
        generated_at: UnixTimestampMilliseconds::new(1_800_000_000_000),
        duration_milliseconds: Some(120_000),
    });
    let ChapterObservationProjection::Qualified { artifact, .. } = projection else {
        panic!("publisher fixture must qualify")
    };
    artifact
}

pub(super) fn open(fixture: &PlaybackFixture, now: i64) -> Arc<Pod0Facade> {
    Pod0Facade::open_with_clock(
        fixture.target.to_string_lossy().into_owned(),
        Arc::new(FixedClock(now)),
    )
}

pub(super) fn dispatch_ensure(facade: &Pod0Facade, episode_id: EpisodeId, id: u64) {
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(70, id),
        cancellation_id: CancellationId::from_parts(71, id),
        expected_revision: None,
        command: ApplicationCommand::EnsurePublisherChapters { episode_id },
    });
}

pub(super) fn one_request(facade: &Pod0Facade) -> HostRequestEnvelope {
    let requests = facade.next_host_requests(64);
    assert_eq!(requests.len(), 1);
    requests.into_iter().next().unwrap()
}

pub(super) fn response(
    request: &HostRequestEnvelope,
    sequence_number: u64,
    http_status: u16,
    bytes: Vec<u8>,
) -> HostObservationEnvelope {
    let HostRequest::FetchPublisherChapters { episode_id, .. } = &request.request else {
        panic!("expected publisher chapter request")
    };
    HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number,
        observed_at: UnixTimestampMilliseconds::new(1),
        observation: HostObservation::PublisherChaptersFetched {
            episode_id: *episode_id,
            bytes,
            content_type: "application/json".to_owned(),
            response_url: "https://example.test/chapters.json".to_owned(),
            entity_tag: None,
            last_modified: None,
            http_status,
        },
    }
}

pub(super) fn valid_document() -> Vec<u8> {
    br#"{"version":"1.2.0","chapters":[
      {"startTime":0,"title":"Opening"},
      {"startTime":60,"title":"Deep dive"}
    ]}"#
    .to_vec()
}

pub(super) fn workflow_request(episode_id: Option<EpisodeId>) -> ProjectionRequest {
    ProjectionRequest {
        scope: ProjectionScope::ChapterWorkflows { episode_id },
        offset: 0,
        max_items: 20,
    }
}

pub(super) fn workflows(
    facade: &Pod0Facade,
    episode_id: Option<EpisodeId>,
) -> ChapterWorkflowsProjection {
    let Projection::ChapterWorkflows { value } =
        facade.snapshot(workflow_request(episode_id)).projection
    else {
        panic!("expected chapter workflow projection")
    };
    value
}

pub(super) fn selected_chapter(
    facade: &Pod0Facade,
    episode_id: EpisodeId,
) -> ChapterArtifactProjection {
    let Projection::Chapter { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Chapter {
                episode_id,
                scope: ChapterProjectionScope::Summary,
            },
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected selected chapter projection")
    };
    value
}
