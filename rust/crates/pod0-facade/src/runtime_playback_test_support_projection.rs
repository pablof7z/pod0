use super::*;

pub(crate) fn dispatch(facade: &Pod0Facade, id: u64, command: PlaybackCommand) {
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(10, id),
        cancellation_id: CancellationId::from_parts(11, id),
        expected_revision: None,
        command: ApplicationCommand::Playback { command },
    });
}

pub(crate) fn playback(facade: &Pod0Facade) -> PlaybackProjection {
    let Projection::Playback { value } = facade.snapshot(playback_request()).projection else {
        panic!("expected playback projection");
    };
    value
}

pub(crate) fn add_external_episode(fixture: &PlaybackFixture, id: u64) -> EpisodeId {
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(10, id),
        cancellation_id: CancellationId::from_parts(11, id),
        expected_revision: None,
        command: ApplicationCommand::UpsertExternalEpisode {
            episode: pod0_application::ExternalEpisodeInput {
                podcast_id: fixture.podcast_id,
                feed_url: None,
                podcast_title: "Legacy Kotlin fixture".to_owned(),
                audio_url: format!("https://legacy.example/{id}.mp3"),
                guid: None,
                title: format!("Episode {id}"),
                description: String::new(),
                published_at: UnixTimestampMilliseconds::new(1_800_000_000_000),
                enclosure_mime_type: Some("audio/mpeg".to_owned()),
                image_url: None,
                duration_milliseconds: Some(180_000),
            },
        },
    });
    let Projection::Library { value } = fixture.facade.snapshot(library_request()).projection
    else {
        panic!("expected library projection");
    };
    value
        .episodes
        .iter()
        .find(|episode| episode.episode_id != fixture.episode_id)
        .unwrap()
        .episode_id
}
