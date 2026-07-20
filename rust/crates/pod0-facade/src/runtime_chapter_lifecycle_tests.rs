use crate::runtime_chapter_tests::{chapter_input, envelope};
use crate::runtime_playback_test_support::{PlaybackFixture, transcript_input};
use crate::*;

#[test]
fn unsubscribe_hides_library_without_deleting_selected_or_historical_chapters() {
    let fixture = PlaybackFixture::new_with_chapters();
    fixture
        .facade
        .dispatch(envelope(60, chapter_input(&fixture, "Replacement"), 1));
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(30, 61),
        cancellation_id: CancellationId::from_parts(31, 61),
        expected_revision: None,
        command: ApplicationCommand::Unsubscribe {
            podcast_id: fixture.podcast_id,
        },
    });

    let Projection::Library { value: library } = fixture
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Library,
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected library projection");
    };
    assert!(library.episodes.is_empty());

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let selected = chapter_for_episode(&reopened, fixture.episode_id);
    assert_eq!(selected.summary.unwrap().selection_revision.value, 2);
    let connection = rusqlite::Connection::open(&fixture.target).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM pod0_chapter_artifacts", [], |row| {
                row.get::<_, u32>(0)
            })
            .unwrap(),
        2
    );
}

#[test]
fn transcript_replacement_requires_a_new_chapter_generation_without_retargeting_history() {
    let fixture = PlaybackFixture::new_with_transcript(true);
    let first_transcript = transcript_summary(&fixture.facade, fixture.episode_id);
    fixture.facade.dispatch(envelope(
        70,
        generated_chapter_input(&fixture, "First generation", &first_transcript),
        0,
    ));
    let first_projection = chapter_for_episode(&fixture.facade, fixture.episode_id);
    let first_chapter = first_projection.summary.unwrap_or_else(|| {
        panic!(
            "first chapter commit failed: {:?}",
            first_projection.operations
        )
    });

    let mut replacement = transcript_input(&fixture);
    replacement.source_revision = "fixture-transcript-v2".to_owned();
    replacement.source_payload_digest = ContentDigest::from_bytes([0x46; 32]);
    replacement.segments[0].text = "Replacement transcript evidence".to_owned();
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(30, 71),
        cancellation_id: CancellationId::from_parts(31, 71),
        expected_revision: None,
        command: ApplicationCommand::CommitTranscript {
            expected_selection_revision: StateRevision::new(1),
            artifact: replacement,
        },
    });
    let second_transcript = transcript_summary(&fixture.facade, fixture.episode_id);
    assert_ne!(
        first_transcript.transcript_version_id,
        second_transcript.transcript_version_id
    );

    fixture.facade.dispatch(envelope(
        72,
        generated_chapter_input(&fixture, "Stale generation", &first_transcript),
        1,
    ));
    assert_eq!(
        chapter_for_episode(&fixture.facade, fixture.episode_id)
            .summary
            .unwrap()
            .artifact_id,
        first_chapter.artifact_id
    );

    fixture.facade.dispatch(envelope(
        73,
        generated_chapter_input(&fixture, "Second generation", &second_transcript),
        1,
    ));
    let selected = chapter_for_episode(&fixture.facade, fixture.episode_id)
        .summary
        .unwrap();
    assert_ne!(selected.artifact_id, first_chapter.artifact_id);
    assert_eq!(
        selected.provenance.transcript_version_id,
        Some(second_transcript.transcript_version_id)
    );

    let connection = rusqlite::Connection::open(&fixture.target).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM pod0_chapter_artifacts", [], |row| {
                row.get::<_, u32>(0)
            })
            .unwrap(),
        2
    );
}

fn generated_chapter_input(
    fixture: &PlaybackFixture,
    first_title: &str,
    transcript: &TranscriptSummaryProjection,
) -> ChapterArtifactInput {
    let mut input = chapter_input(fixture, first_title);
    input.source_revision = format!("generated-{first_title}");
    input.provenance = ChapterArtifactProvenance {
        source: ChapterArtifactSource::Generated,
        provider: Some("fixture-provider".to_owned()),
        model: Some("fixture-model".to_owned()),
        policy_version: 1,
        source_payload_digest: ContentDigest::from_bytes([first_title.len() as u8; 32]),
        transcript_version_id: Some(transcript.transcript_version_id),
        transcript_content_digest: Some(transcript.transcript_content_digest),
        legacy_import: None,
    };
    input
}

fn chapter_for_episode(facade: &Pod0Facade, episode_id: EpisodeId) -> ChapterArtifactProjection {
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
        panic!("expected chapter projection");
    };
    value
}

fn transcript_summary(facade: &Pod0Facade, episode_id: EpisodeId) -> TranscriptSummaryProjection {
    let Projection::Transcript { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Transcript {
                episode_id,
                scope: TranscriptProjectionScope::Summary,
            },
            offset: 0,
            max_items: 1,
        })
        .projection
    else {
        panic!("expected transcript projection");
    };
    value.summary.unwrap()
}
