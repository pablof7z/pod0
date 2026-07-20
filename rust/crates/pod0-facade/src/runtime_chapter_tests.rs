use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn chapter_commit_is_durable_bounded_and_replay_safe_across_restart() {
    let fixture = PlaybackFixture::new_with_chapters();
    let command = envelope(40, chapter_input(&fixture, "Fresh chapter"), 1);
    fixture.facade.dispatch(command.clone());

    let summary = chapter(&fixture.facade, ChapterProjectionScope::Summary, 0, 20);
    assert_eq!(
        summary.summary.as_ref().unwrap().selection_revision.value,
        2
    );
    assert_eq!(summary.summary.as_ref().unwrap().chapter_count, 2);
    assert!(matches!(
        summary
            .operations
            .iter()
            .find(|operation| operation.command_id == command.command_id)
            .and_then(|operation| operation.result),
        Some(OperationResult::ChapterCommitted { receipt })
            if receipt.selection_revision.value == 2 && receipt.chapter_count == 2
    ));

    let first_page = chapter(&fixture.facade, ChapterProjectionScope::Chapters, 0, 1);
    assert_eq!(first_page.chapters.len(), 1);
    assert!(first_page.has_more);
    let second_page = chapter(&fixture.facade, ChapterProjectionScope::Chapters, 1, 1);
    assert_eq!(second_page.chapters.len(), 1);
    assert!(!second_page.has_more);

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    reopened.dispatch(command);
    let replayed = chapter(&reopened, ChapterProjectionScope::Summary, 0, 20);
    assert_eq!(replayed.summary.unwrap().selection_revision.value, 2);
    assert!(matches!(
        replayed.operations.last().and_then(|operation| operation.result),
        Some(OperationResult::ChapterCommitted { receipt })
            if receipt.selection_revision.value == 2
    ));
}

#[test]
fn stale_revision_and_conflicting_command_reuse_cannot_replace_selection() {
    let fixture = PlaybackFixture::new_with_chapters();
    let committed = envelope(50, chapter_input(&fixture, "Selected"), 1);
    fixture.facade.dispatch(committed.clone());

    let stale = envelope(51, chapter_input(&fixture, "Stale"), 1);
    fixture.facade.dispatch(stale.clone());
    let after_stale = chapter(&fixture.facade, ChapterProjectionScope::Chapters, 0, 20);
    assert_eq!(after_stale.chapters[0].title, "Selected");
    assert!(matches!(
        after_stale
            .operations
            .iter()
            .find(|operation| operation.command_id == stale.command_id),
        Some(OperationProjection {
            stage: OperationStage::Failed,
            failure: Some(CoreFailure {
                code: CoreFailureCode::RevisionConflict,
                ..
            }),
            ..
        })
    ));

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    reopened.dispatch(envelope(50, chapter_input(&fixture, "Conflict"), 1));
    let after_conflict = chapter(&reopened, ChapterProjectionScope::Chapters, 0, 20);
    assert_eq!(after_conflict.chapters[0].title, "Selected");
    assert!(matches!(
        after_conflict.operations.last(),
        Some(OperationProjection {
            stage: OperationStage::Failed,
            failure: Some(CoreFailure {
                code: CoreFailureCode::InvalidCommand,
                ..
            }),
            ..
        })
    ));
}

pub(super) fn chapter_input(fixture: &PlaybackFixture, first_title: &str) -> ChapterArtifactInput {
    ChapterArtifactInput {
        episode_id: fixture.episode_id,
        podcast_id: fixture.podcast_id,
        source_revision: format!("publisher-{first_title}"),
        provenance: ChapterArtifactProvenance {
            source: ChapterArtifactSource::Publisher,
            provider: Some("fixture-publisher".to_owned()),
            model: None,
            policy_version: 0,
            source_payload_digest: ContentDigest::from_bytes([first_title.len() as u8; 32]),
            transcript_version_id: None,
            transcript_content_digest: None,
            legacy_import: None,
        },
        generated_at: UnixTimestampMilliseconds::new(1_800_000_000_100),
        duration_milliseconds: Some(120_000),
        chapters: vec![
            ChapterInput {
                start_milliseconds: 0,
                end_milliseconds: Some(60_000),
                title: first_title.to_owned(),
                summary: None,
                image_url: None,
                link_url: None,
                include_in_table_of_contents: true,
                source_episode_id: None,
            },
            ChapterInput {
                start_milliseconds: 60_000,
                end_milliseconds: None,
                title: "Second".to_owned(),
                summary: Some("Bounded projection evidence".to_owned()),
                image_url: None,
                link_url: None,
                include_in_table_of_contents: true,
                source_episode_id: None,
            },
        ],
        ad_span_evaluation: AdSpanEvaluation::Evaluated,
        ad_spans: vec![AdSpanInput {
            start_milliseconds: 20_000,
            end_milliseconds: 30_000,
            kind: ChapterAdKind::Midroll,
        }],
    }
}

pub(super) fn envelope(
    id: u64,
    artifact: ChapterArtifactInput,
    expected_revision: u64,
) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(30, id),
        cancellation_id: CancellationId::from_parts(31, id),
        expected_revision: None,
        command: ApplicationCommand::CommitChapter {
            expected_selection_revision: StateRevision::new(expected_revision),
            artifact,
        },
    }
}

fn chapter(
    facade: &Pod0Facade,
    scope: ChapterProjectionScope,
    offset: u32,
    max_items: u16,
) -> ChapterArtifactProjection {
    let Projection::Chapter { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Chapter {
                episode_id: facade
                    .snapshot(ProjectionRequest {
                        scope: ProjectionScope::Library,
                        offset: 0,
                        max_items: 1,
                    })
                    .projection
                    .library_episode_id(),
                scope,
            },
            offset,
            max_items,
        })
        .projection
    else {
        panic!("expected chapter projection");
    };
    value
}

trait LibraryEpisodeId {
    fn library_episode_id(&self) -> EpisodeId;
}

impl LibraryEpisodeId for Projection {
    fn library_episode_id(&self) -> EpisodeId {
        let Projection::Library { value } = self else {
            panic!("expected library projection");
        };
        value.episodes[0].episode_id
    }
}
