use crate::runtime_chapter_workflow_test_support::{
    dispatch_ensure, one_request, open, response, set_source, valid_document, workflows,
};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

mod recovery;

fn fixture() -> PlaybackFixture {
    let fixture = PlaybackFixture::new_with_transcript(true);
    set_source(&fixture, None);
    fixture
}

fn ensure(facade: &Pod0Facade, episode_id: EpisodeId, command: u64) {
    ensure_with_model(facade, episode_id, command, "ollama:llama3.2");
}

fn ensure_with_model(
    facade: &Pod0Facade,
    episode_id: EpisodeId,
    command: u64,
    configured_model: &str,
) {
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(80, command),
        cancellation_id: CancellationId::from_parts(81, command),
        expected_revision: None,
        command: ApplicationCommand::EnsureModelChapters {
            episode_id,
            configured_model: configured_model.into(),
        },
    });
}

fn model_request(facade: &Pod0Facade) -> HostRequestEnvelope {
    let requests = facade.next_host_requests(64);
    requests
        .into_iter()
        .find(|request| {
            matches!(
                request.request,
                HostRequest::ExecuteChapterModel { .. }
                    | HostRequest::RecoverChapterModelOperation { .. }
            )
        })
        .expect("model request")
}

fn completion(request: &HostRequestEnvelope) -> HostObservationEnvelope {
    model_completion(
        request,
        r#"{"chapters":[{"start":0,"title":"Opening"},{"start":30,"title":"Context"},{"start":60,"title":"Deep dive"},{"start":90,"title":"Close"}],"ads":[]}"#,
        "llama3.2:latest",
    )
}

fn model_completion(
    request: &HostRequestEnvelope,
    completion: &str,
    resolved_model: &str,
) -> HostObservationEnvelope {
    let (episode_id, generation, submission_fence_id) = match request.request {
        HostRequest::ExecuteChapterModel {
            episode_id,
            generation,
            submission_fence_id,
            ..
        }
        | HostRequest::RecoverChapterModelOperation {
            episode_id,
            generation,
            submission_fence_id,
            ..
        } => (episode_id, generation, submission_fence_id),
        _ => panic!("expected model request"),
    };
    HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: 1,
        observed_at: UnixTimestampMilliseconds::new(1_800_000_100_000),
        observation: HostObservation::ChapterModelCompleted {
            episode_id,
            generation,
            submission_fence_id,
            completion: ChapterModelCompletionObservation {
                completion: completion.into(),
                provider: "ollama".into(),
                model: resolved_model.into(),
                prompt_tokens: Some(100),
                completion_tokens: Some(50),
                cached_tokens: Some(0),
                reasoning_tokens: Some(0),
                cost_microusd: None,
                provider_operation_id: None,
                provider_status: Some("completed".into()),
                provider_generated_at: Some(UnixTimestampMilliseconds::new(1_800_000_099_000)),
            },
        },
    }
}

#[test]
fn changed_model_reenriches_the_original_publisher_artifact() {
    let fixture = PlaybackFixture::new_with_transcript(true);
    set_source(&fixture, Some("https://example.test/chapters.json"));
    dispatch_ensure(&fixture.facade, fixture.episode_id, 90);
    let publisher_request = one_request(&fixture.facade);
    fixture
        .facade
        .record_host_observation(response(&publisher_request, 1, 200, valid_document()));
    let publisher_id = workflows(&fixture.facade, Some(fixture.episode_id)).publisher[0]
        .selected_artifact_id
        .expect("publisher artifact");

    ensure(&fixture.facade, fixture.episode_id, 91);
    let first = model_request(&fixture.facade);
    let first_record = fixture
        .facade
        .state()
        .store
        .as_ref()
        .unwrap()
        .model_chapter_workflow(fixture.episode_id)
        .unwrap()
        .unwrap();
    let first_request = first_record.active_request.as_ref().unwrap();
    assert_eq!(
        first_request.mode,
        pod0_storage::ModelChapterWorkflowMode::Enrich
    );
    assert_eq!(first_request.base_artifact_id, Some(publisher_id));
    fixture.facade.record_host_observation(model_completion(
        &first,
        r#"{"summaries":[{"index":0,"summary":"First model"}],"ads":[]}"#,
        "llama3.2:latest",
    ));

    ensure(&fixture.facade, fixture.episode_id, 92);
    assert!(fixture.facade.next_host_requests(64).is_empty());

    ensure_with_model(&fixture.facade, fixture.episode_id, 93, "ollama:qwen3:8b");
    let _replanned = model_request(&fixture.facade);
    let replanned_record = fixture
        .facade
        .state()
        .store
        .as_ref()
        .unwrap()
        .model_chapter_workflow(fixture.episode_id)
        .unwrap()
        .unwrap();
    let replanned_request = replanned_record.active_request.as_ref().unwrap();
    assert_eq!(
        replanned_request.mode,
        pod0_storage::ModelChapterWorkflowMode::Enrich
    );
    assert_eq!(replanned_request.base_artifact_id, Some(publisher_id));
    let base = fixture
        .facade
        .state()
        .store
        .as_ref()
        .unwrap()
        .chapter_artifact(publisher_id)
        .unwrap()
        .unwrap();
    assert_eq!(base.artifact_id, publisher_id);
    assert_eq!(base.provenance.source, ChapterArtifactSource::Publisher);
    assert!(
        base.chapters
            .iter()
            .all(|chapter| chapter.summary.is_none())
    );
}

#[test]
fn model_request_is_claimed_before_delivery_and_completion_has_durable_ack() {
    let fixture = fixture();
    ensure(&fixture.facade, fixture.episode_id, 1);
    let before = fixture
        .facade
        .state()
        .store
        .as_ref()
        .unwrap()
        .model_chapter_workflow(fixture.episode_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        before.state,
        pod0_storage::ModelChapterWorkflowState::Requested
    );

    let request = model_request(&fixture.facade);
    let claimed = fixture
        .facade
        .state()
        .store
        .as_ref()
        .unwrap()
        .model_chapter_workflow(fixture.episode_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        claimed.state,
        pod0_storage::ModelChapterWorkflowState::SubmissionAuthorized
    );
    assert!(fixture.facade.next_host_requests(64).is_empty());

    let observation = completion(&request);
    assert_eq!(
        fixture.facade.record_host_observation(observation.clone()),
        HostObservationReceipt::Persisted {
            request_id: request.request_id,
            terminal: true,
        }
    );
    assert_eq!(
        fixture.facade.record_host_observation(observation),
        HostObservationReceipt::Persisted {
            request_id: request.request_id,
            terminal: true,
        }
    );
    let projection = workflows(&fixture.facade, Some(fixture.episode_id));
    assert_eq!(
        projection.model[0].stage,
        ModelChapterWorkflowStage::Succeeded
    );
    let selected = fixture
        .facade
        .state()
        .store
        .as_ref()
        .unwrap()
        .selected_chapter_artifact(fixture.episode_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        selected.artifact.provenance.model.as_deref(),
        Some("llama3.2:latest")
    );

    let completed = workflows(&fixture.facade, Some(fixture.episode_id)).model[0].clone();
    assert_eq!(
        selected.artifact.source_revision,
        completed.source_version.clone().unwrap()
    );
    ensure(&fixture.facade, fixture.episode_id, 9);
    assert!(fixture.facade.next_host_requests(64).is_empty());
    let adopted = workflows(&fixture.facade, Some(fixture.episode_id)).model[0].clone();
    assert_eq!(adopted.stage, ModelChapterWorkflowStage::Succeeded);
    assert_eq!(adopted.generation, completed.generation);
    assert_eq!(adopted.selected_artifact_id, completed.selected_artifact_id);

    let reopened = open(&fixture, 1_800_000_200_000);
    assert_eq!(
        workflows(&reopened, Some(fixture.episode_id)).model[0].stage,
        ModelChapterWorkflowStage::Succeeded
    );
    ensure(&reopened, fixture.episode_id, 10);
    assert!(
        reopened.next_host_requests(64).is_empty(),
        "reopening and re-ensuring an exact durable completion must never spend twice"
    );
}
