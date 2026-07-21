use sha2::Digest as _;

use crate::model_chapter_cutover_tests::fixture_without_model_authority;
use crate::runtime_chapter_workflow_test_support::workflows;
use crate::*;

#[test]
fn exact_legacy_success_adopts_the_already_selected_model_artifact() {
    let fixture = fixture_without_model_authority();
    let facade = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let ChapterModelPlan::Ready { request } =
        facade.plan_chapter_model_request(fixture.episode_id, "ollama:llama3.2".into())
    else {
        panic!("fixture must initially produce a model request")
    };
    let completion = r#"{"chapters":[{"start":0,"title":"Opening"},{"start":30,"title":"Context"},{"start":60,"title":"Deep dive"},{"start":90,"title":"Close"}],"ads":[]}"#;
    let ChapterObservationProjection::Qualified { artifact, .. } =
        qualify_model_chapter_observation(ModelChapterObservation {
            episode_id: fixture.episode_id,
            podcast_id: fixture.podcast_id,
            format_version: request.format_version,
            requested_transcript_version_id: request.requested_transcript_version_id,
            requested_transcript_content_digest: request.requested_transcript_content_digest,
            selected_transcript_version_id: request.selected_transcript_version_id,
            selected_transcript_content_digest: request.selected_transcript_content_digest,
            policy_version: request.policy_version,
            source_version: request.source_version.clone(),
            provider: request.provider.clone(),
            model: request.model.clone(),
            completion_digest: ContentDigest::from_bytes(sha2::Sha256::digest(completion).into()),
            completion: completion.into(),
            generated_at: UnixTimestampMilliseconds::new(1_800_000_100_000),
            duration_milliseconds: request.duration_milliseconds,
            mode: ChapterModelObservationMode::Generate,
        })
    else {
        panic!("fixture completion must qualify")
    };
    let receipt = facade
        .state()
        .store
        .as_ref()
        .unwrap()
        .commit_and_select_chapter(
            CommandId::from_parts(106, 1),
            StateRevision::INITIAL,
            artifact,
            1_800_000_100_001,
        )
        .unwrap();
    assert_eq!(
        facade.plan_chapter_model_request(fixture.episode_id, "ollama:llama3.2".into()),
        ChapterModelPlan::Current {
            artifact_id: receipt.artifact_id
        }
    );

    let skipped_ambiguity = facade.stage_legacy_model_chapter_cutover(
        58,
        "ollama:llama3.2".into(),
        vec![LegacyModelChapterCutoverCandidate {
            episode_id: fixture.episode_id,
            input_version: request.source_version.clone(),
            disposition: LegacyModelChapterCutoverDisposition::Ambiguous,
        }],
    );
    assert_eq!(
        skipped_ambiguity.stage,
        LegacyModelChapterCutoverStage::Staged
    );
    assert_eq!(skipped_ambiguity.adopted_ambiguous, 0);
    assert!(
        workflows(&facade, Some(fixture.episode_id))
            .model
            .is_empty()
    );
    assert_eq!(
        facade.discard_staged_legacy_model_chapter_cutover(58).stage,
        LegacyModelChapterCutoverStage::NotStarted
    );

    let staged = facade.stage_legacy_model_chapter_cutover(
        59,
        "ollama:llama3.2".into(),
        vec![LegacyModelChapterCutoverCandidate {
            episode_id: fixture.episode_id,
            input_version: request.source_version,
            disposition: LegacyModelChapterCutoverDisposition::Succeeded {
                artifact_id: receipt.artifact_id,
                content_digest: receipt.content_digest,
                integrity_digest: receipt.integrity_digest,
                selection_revision: receipt.selection_revision,
            },
        }],
    );
    assert_eq!(staged.stage, LegacyModelChapterCutoverStage::Staged);
    assert_eq!(staged.adopted_succeeded, 1);
    let adopted = workflows(&facade, Some(fixture.episode_id)).model;
    assert_eq!(adopted.len(), 1);
    assert_eq!(adopted[0].stage, ModelChapterWorkflowStage::Succeeded);
    assert_eq!(adopted[0].selected_artifact_id, Some(receipt.artifact_id));
    assert!(facade.next_host_requests(64).is_empty());
}
