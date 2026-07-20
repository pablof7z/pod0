use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn facade_plans_from_authoritative_episode_transcript_and_chapter_state() {
    let fixture = PlaybackFixture::new_with_transcript_and_chapters();
    let expected_selection_revision = fixture
        .facade
        .state()
        .store
        .as_ref()
        .unwrap()
        .selected_chapter_artifact(fixture.episode_id)
        .unwrap()
        .unwrap()
        .selection_revision;

    let ChapterModelPlan::Ready { request } = fixture
        .facade
        .plan_chapter_model_request(fixture.episode_id, "ollama:llama3.2".into())
    else {
        panic!("authoritative publisher chapter and transcript must be ready")
    };

    assert_eq!(request.episode_id, fixture.episode_id);
    assert_eq!(request.podcast_id, fixture.podcast_id);
    assert_eq!(request.provider, "ollama");
    assert_eq!(request.model, "llama3.2");
    assert_eq!(
        request.expected_chapter_selection_revision,
        expected_selection_revision
    );
    assert!(matches!(
        request.mode,
        ChapterModelObservationMode::Enrich { .. }
    ));
    assert_eq!(
        request.expected_artifact_source,
        ChapterArtifactSource::PublisherEnriched
    );
    assert!(request.user_prompt.contains("Fixture transcript evidence"));
    assert!(request.user_prompt.contains("[0] 0s — Opening"));
}

#[test]
fn facade_returns_typed_unavailable_states_without_native_state_reconstruction() {
    let fixture = PlaybackFixture::new();
    assert_eq!(
        fixture
            .facade
            .plan_chapter_model_request(fixture.episode_id, "model".into()),
        ChapterModelPlan::TranscriptUnavailable
    );
    assert_eq!(
        fixture
            .facade
            .plan_chapter_model_request(EpisodeId::from_parts(99, 99), "model".into()),
        ChapterModelPlan::EpisodeUnavailable
    );
}
