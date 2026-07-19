use std::path::PathBuf;
use std::sync::Arc;

use crate::*;

pub(super) struct PlaybackFixture {
    _directory: tempfile::TempDir,
    pub(super) target: PathBuf,
    pub(super) facade: Arc<Pod0Facade>,
    pub(super) episode_id: EpisodeId,
    pub(super) podcast_id: PodcastId,
}

impl PlaybackFixture {
    pub(super) fn new() -> Self {
        Self::new_with_transcript(false)
    }

    pub(super) fn new_with_transcript(transcript_available: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let canonical_source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../Fixtures/CoreImport/legacy-listening-v1.json");
        let source = if transcript_available {
            let original = std::fs::read_to_string(&canonical_source).unwrap();
            let modified = original.replace(
                "\"transcriptState\": {\"none\": {}}",
                "\"transcriptState\": {\"ready\": {\"source\": \"publisher\"}}",
            );
            assert_ne!(modified, original);
            let source = directory.path().join("legacy-with-transcript.json");
            std::fs::write(&source, modified).unwrap();
            source
        } else {
            canonical_source
        };
        let source_backup = directory.path().join("legacy.backup.json");
        let target = directory.path().join("core.sqlite");
        let schema_backup = directory.path().join("core.backup.sqlite");
        let plan = inspect_legacy_listening_source(source.to_string_lossy().into_owned()).unwrap();
        stage_legacy_listening_import(
            source.to_string_lossy().into_owned(),
            source_backup.to_string_lossy().into_owned(),
            target.to_string_lossy().into_owned(),
            schema_backup.to_string_lossy().into_owned(),
            plan,
            CommandId::from_parts(9, 1),
            CommandId::from_parts(9, 2),
            1_800_000_000_000,
        )
        .unwrap();
        commit_staged_legacy_listening_import(
            target.to_string_lossy().into_owned(),
            1_800_000_000_001,
        )
        .unwrap();
        let facade = Pod0Facade::open(target.to_string_lossy().into_owned()).unwrap();
        let Projection::Library { value } = facade.snapshot(library_request()).projection else {
            panic!("expected library projection");
        };
        Self {
            _directory: directory,
            target,
            episode_id: value.episodes[0].episode_id,
            podcast_id: value.podcasts[0].podcast_id,
            facade,
        }
    }

    pub(super) fn dispatch(&self, id: u64, command: PlaybackCommand) {
        dispatch(&self.facade, id, command);
    }

    pub(super) fn playback(&self) -> PlaybackProjection {
        playback(&self.facade)
    }
}

pub(super) fn dispatch(facade: &Pod0Facade, id: u64, command: PlaybackCommand) {
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(10, id),
        cancellation_id: CancellationId::from_parts(11, id),
        expected_revision: None,
        command: ApplicationCommand::Playback { command },
    });
}

pub(super) fn playback(facade: &Pod0Facade) -> PlaybackProjection {
    let Projection::Playback { value } = facade.snapshot(playback_request()).projection else {
        panic!("expected playback projection");
    };
    value
}

pub(super) fn add_external_episode(fixture: &PlaybackFixture, id: u64) -> EpisodeId {
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

pub(super) fn record_playback(
    facade: &Pod0Facade,
    stream: &HostRequestEnvelope,
    sequence_number: u64,
    observed_at: i64,
    position: u64,
    ended: bool,
    interruption: PlaybackInterruption,
) {
    record_observation(
        facade,
        stream,
        sequence_number,
        observed_at,
        PlaybackLifecycleObservation {
            episode_id: playback(facade).current.map(|item| item.episode_id),
            state: if ended {
                PlaybackHostState::Paused
            } else {
                PlaybackHostState::Playing
            },
            position_milliseconds: position,
            duration_milliseconds: 120_500,
            route: PlaybackAudioRoute::BuiltIn,
            interruption,
            ended,
        },
    );
}

pub(super) fn record_observation(
    facade: &Pod0Facade,
    request: &HostRequestEnvelope,
    sequence_number: u64,
    observed_at: i64,
    value: PlaybackLifecycleObservation,
) {
    facade.record_host_observation(HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number,
        observed_at: UnixTimestampMilliseconds::new(observed_at),
        observation: HostObservation::PlaybackObserved { value },
    });
}

pub(super) fn library_request() -> ProjectionRequest {
    ProjectionRequest {
        scope: ProjectionScope::Library,
        offset: 0,
        max_items: 200,
    }
}

pub(super) fn playback_request() -> ProjectionRequest {
    ProjectionRequest {
        scope: ProjectionScope::Playback,
        offset: 0,
        max_items: 200,
    }
}
