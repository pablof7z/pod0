use crate::runtime_chapter_workflow_test_support::{dispatch_ensure, set_source};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn publisher_and_model_workflows_share_one_projection_bound() {
    let fixture = PlaybackFixture::new_with_transcript(true);
    set_source(&fixture, Some("https://example.test/chapters.json"));
    dispatch_ensure(&fixture.facade, fixture.episode_id, 20);
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(84, 1),
        cancellation_id: CancellationId::from_parts(85, 1),
        expected_revision: None,
        command: ApplicationCommand::EnsureModelChapters {
            episode_id: fixture.episode_id,
            configured_model: "ollama:llama3.2".into(),
        },
    });

    let projection = fixture.facade.snapshot(ProjectionRequest {
        scope: ProjectionScope::ChapterWorkflows {
            episode_id: Some(fixture.episode_id),
        },
        offset: 0,
        max_items: 1,
    });
    let Projection::ChapterWorkflows { value } = projection.projection else {
        panic!("expected chapter workflow projection")
    };
    assert_eq!(value.publisher.len() + value.model.len(), 1);
    assert!(value.has_more);
}
