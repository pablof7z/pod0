use std::path::PathBuf;
use std::sync::Arc;

use rusqlite::Connection;

use crate::*;

#[path = "runtime_playback_observation_test_support.rs"]
mod observations;
pub(super) use observations::*;
#[path = "runtime_chapter_playback_test_support.rs"]
mod chapters;
use chapters::{install_chapter_fixture, install_empty_chapter_fixture};

pub(super) struct PlaybackFixture {
    _directory: tempfile::TempDir,
    pub(super) target: PathBuf,
    pub(super) facade: Arc<Pod0Facade>,
    pub(super) episode_id: EpisodeId,
    pub(super) podcast_id: PodcastId,
}

impl PlaybackFixture {
    pub(super) fn new() -> Self {
        Self::new_with_options(false, false)
    }

    pub(super) fn new_with_transcript(transcript_available: bool) -> Self {
        Self::new_with_options(transcript_available, false)
    }

    pub(super) fn new_with_chapters() -> Self {
        Self::new_with_options(false, true)
    }

    pub(super) fn new_with_transcript_and_chapters() -> Self {
        Self::new_with_options(true, true)
    }

    fn new_with_options(transcript_available: bool, chapters_available: bool) -> Self {
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
        let note_plan = inspect_legacy_note_source(source.to_string_lossy().into_owned()).unwrap();
        stage_legacy_note_import(
            source.to_string_lossy().into_owned(),
            directory
                .path()
                .join("legacy-notes.backup.json")
                .to_string_lossy()
                .into_owned(),
            target.to_string_lossy().into_owned(),
            schema_backup.to_string_lossy().into_owned(),
            note_plan,
            CommandId::from_parts(9, 3),
            CommandId::from_parts(9, 2),
            1_800_000_000_002,
        )
        .unwrap();
        commit_staged_legacy_note_import(target.to_string_lossy().into_owned(), 1_800_000_000_003)
            .unwrap();
        let clip_plan = inspect_legacy_clip_source(source.to_string_lossy().into_owned()).unwrap();
        stage_legacy_clip_import(
            source.to_string_lossy().into_owned(),
            directory
                .path()
                .join("legacy-clips.backup.json")
                .to_string_lossy()
                .into_owned(),
            target.to_string_lossy().into_owned(),
            schema_backup.to_string_lossy().into_owned(),
            clip_plan,
            CommandId::from_parts(9, 4),
            CommandId::from_parts(9, 2),
            1_800_000_000_004,
        )
        .unwrap();
        commit_staged_legacy_clip_import(
            source.to_string_lossy().into_owned(),
            target.to_string_lossy().into_owned(),
            1_800_000_000_005,
        )
        .unwrap();
        let transcript_source = directory.path().join("legacy-transcripts.sqlite");
        Connection::open(&transcript_source)
            .unwrap()
            .execute_batch(
                "CREATE TABLE artifacts(\
                 id INTEGER PRIMARY KEY AUTOINCREMENT,kind TEXT NOT NULL,subject_id TEXT NOT NULL,\
                 input_version TEXT NOT NULL,output_version TEXT NOT NULL,content_hash TEXT NOT NULL,\
                 location TEXT,origin TEXT,schema_version INTEGER NOT NULL,integrity TEXT NOT NULL,\
                 verified_at REAL NOT NULL,selected INTEGER NOT NULL,\
                 UNIQUE(kind,subject_id,input_version,output_version));\
                 CREATE TABLE workflow_schema_versions(component TEXT PRIMARY KEY,version INTEGER NOT NULL);\
                 INSERT INTO workflow_schema_versions VALUES('artifacts',1);",
            )
            .unwrap();
        let transcript_root = directory.path().join("legacy-transcript-artifacts");
        let transcript_backup = directory.path().join("legacy-transcript-backups");
        std::fs::create_dir_all(&transcript_root).unwrap();
        let transcript_plan = inspect_legacy_transcript_source(
            transcript_source.to_string_lossy().into_owned(),
            transcript_root.to_string_lossy().into_owned(),
        )
        .unwrap();
        stage_legacy_transcript_import(
            transcript_source.to_string_lossy().into_owned(),
            transcript_root.to_string_lossy().into_owned(),
            transcript_backup.to_string_lossy().into_owned(),
            target.to_string_lossy().into_owned(),
            schema_backup.to_string_lossy().into_owned(),
            transcript_plan,
            CommandId::from_parts(9, 5),
            CommandId::from_parts(9, 2),
            1_800_000_000_006,
        )
        .unwrap();
        verify_staged_legacy_transcript_import(
            target.to_string_lossy().into_owned(),
            transcript_backup.to_string_lossy().into_owned(),
            CommandId::from_parts(9, 5),
            1_800_000_000_007,
        )
        .unwrap();
        commit_staged_legacy_transcript_import(
            transcript_source.to_string_lossy().into_owned(),
            transcript_root.to_string_lossy().into_owned(),
            target.to_string_lossy().into_owned(),
            CommandId::from_parts(9, 5),
            1_800_000_000_008,
        )
        .unwrap();
        if chapters_available {
            install_chapter_fixture(&directory, &target);
        } else {
            install_empty_chapter_fixture(&directory, &target);
        }
        let facade = Pod0Facade::open(target.to_string_lossy().into_owned()).unwrap();
        let Projection::Library { value } = facade.snapshot(library_request()).projection else {
            panic!("expected library projection");
        };
        let fixture = Self {
            _directory: directory,
            target,
            episode_id: value.episodes[0].episode_id,
            podcast_id: value.podcasts[0].podcast_id,
            facade,
        };
        if transcript_available {
            fixture.facade.dispatch(CommandEnvelope {
                command_id: CommandId::from_parts(9, 6),
                cancellation_id: CancellationId::from_parts(9, 7),
                expected_revision: None,
                command: ApplicationCommand::CommitTranscript {
                    expected_selection_revision: StateRevision::INITIAL,
                    artifact: transcript_input(&fixture),
                },
            });
        }
        fixture
    }

    pub(super) fn dispatch(&self, id: u64, command: PlaybackCommand) {
        dispatch(&self.facade, id, command);
    }

    pub(super) fn playback(&self) -> PlaybackProjection {
        playback(&self.facade)
    }
}

pub(super) fn transcript_input(fixture: &PlaybackFixture) -> TranscriptArtifactInput {
    TranscriptArtifactInput {
        episode_id: fixture.episode_id,
        podcast_id: fixture.podcast_id,
        source_revision: "fixture-transcript-v1".to_owned(),
        source: TranscriptSource::Publisher,
        provider: Some("fixture".to_owned()),
        source_payload_digest: ContentDigest::from_bytes([0x45; 32]),
        language: "en-US".to_owned(),
        generated_at: UnixTimestampMilliseconds::new(1_800_000_000_009),
        speakers: Vec::new(),
        segments: vec![TranscriptArtifactSegmentInput {
            text: "Fixture transcript evidence".to_owned(),
            start_milliseconds: 0,
            end_milliseconds: 1_000,
            speaker_id: None,
            words: Vec::new(),
        }],
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
