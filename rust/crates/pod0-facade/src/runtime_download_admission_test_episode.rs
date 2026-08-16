use super::*;

pub(super) fn add_episode(fixture: &PlaybackFixture, id: u64, published_at: i64) -> EpisodeId {
    let audio_url = format!("https://automatic.example/{id}.mp3");
    dispatch(
        &fixture.facade,
        100 + id,
        ApplicationCommand::UpsertExternalEpisode {
            episode: pod0_application::ExternalEpisodeInput {
                podcast_id: fixture.podcast_id,
                feed_url: None,
                podcast_title: "Automatic fixture".to_owned(),
                audio_url: audio_url.clone(),
                guid: None,
                title: format!("Automatic episode {id}"),
                description: String::new(),
                published_at: UnixTimestampMilliseconds::new(published_at),
                enclosure_mime_type: Some("audio/mpeg".to_owned()),
                image_url: None,
                duration_milliseconds: Some(120_000),
            },
        },
    );
    let Projection::Library { value } = fixture
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Library,
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected library projection")
    };
    value
        .episodes
        .iter()
        .find(|episode| episode.enclosure_url == audio_url)
        .unwrap()
        .episode_id
}
