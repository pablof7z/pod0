use crate::runtime_chapter_workflow_test_support::*;
use crate::*;
use pod0_application::ExternalEpisodeInput;

#[test]
fn publisher_workflow_admission_is_bounded_and_replenishes_after_completion() {
    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_580_000);
    for index in 0_u64..12 {
        facade.dispatch(CommandEnvelope {
            command_id: CommandId::from_parts(100, index + 1),
            cancellation_id: CancellationId::from_parts(101, index + 1),
            expected_revision: None,
            command: ApplicationCommand::UpsertExternalEpisode {
                episode: ExternalEpisodeInput {
                    podcast_id: fixture.podcast_id,
                    feed_url: None,
                    podcast_title: "Admission fixture".to_owned(),
                    audio_url: format!("https://example.test/audio-{index}.mp3"),
                    title: format!("Admission episode {index}"),
                    description: String::new(),
                    published_at: UnixTimestampMilliseconds::new(1_800_000_000_000 + index as i64),
                    enclosure_mime_type: Some("audio/mpeg".to_owned()),
                    image_url: None,
                    duration_milliseconds: Some(120_000),
                },
            },
        });
    }
    let Projection::Library { value } = facade
        .snapshot(crate::runtime_playback_test_support::library_request())
        .projection
    else {
        panic!("expected library projection")
    };
    let episodes = value
        .episodes
        .into_iter()
        .filter(|episode| episode.title.starts_with("Admission episode"))
        .collect::<Vec<_>>();
    assert_eq!(episodes.len(), 12);
    for (index, episode) in episodes.iter().enumerate() {
        set_source_for(
            &fixture,
            episode.episode_id,
            Some(&format!("https://example.test/chapters-{index}.json")),
        );
        dispatch_ensure(&facade, episode.episode_id, 120 + index as u64);
    }

    let admitted = facade.next_host_requests(64);
    assert_eq!(
        admitted.len(),
        usize::from(pod0_application::MAX_ACTIVE_PUBLISHER_CHAPTER_REQUESTS)
    );
    assert!(facade.next_host_requests(64).is_empty());
    assert_eq!(workflows(&facade, None).publisher.len(), 12);

    facade.record_host_observation(response(&admitted[0], 1, 404, Vec::new()));
    assert_eq!(facade.next_host_requests(64).len(), 1);
}
