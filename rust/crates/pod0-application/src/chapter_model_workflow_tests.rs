use pod0_domain::{
    ChapterArtifactSource, ContentDigest, EpisodeId, PodcastId, StateRevision, TranscriptVersionId,
};

use crate::chapter_model_policy_tests::{plan_input, publisher_artifact};
use crate::*;

fn generated_request() -> PlannedChapterModelRequest {
    let ChapterModelPlan::Ready { request } = plan_chapter_model_request(plan_input(None)) else {
        panic!("generation fixture must plan")
    };
    request
}

fn fingerprint(request: &PlannedChapterModelRequest) -> ContentDigest {
    chapter_model_request_fingerprint(request, "openai/gpt-4o-mini").unwrap()
}

#[test]
fn fingerprint_is_deterministic_and_covers_every_request_field() {
    let baseline = generated_request();
    let expected = fingerprint(&baseline);
    assert_eq!(fingerprint(&baseline), expected);

    macro_rules! changed {
        ($mutation:expr) => {{
            let mut request = baseline.clone();
            $mutation(&mut request);
            assert_ne!(fingerprint(&request), expected, "{}", stringify!($mutation));
        }};
    }
    changed!(|r: &mut PlannedChapterModelRequest| r.source_version.push('2'));
    changed!(|r: &mut PlannedChapterModelRequest| r.episode_id = EpisodeId::from_parts(9, 1));
    changed!(|r: &mut PlannedChapterModelRequest| r.podcast_id = PodcastId::from_parts(9, 2));
    changed!(|r: &mut PlannedChapterModelRequest| r.format_version += 1);
    changed!(
        |r: &mut PlannedChapterModelRequest| r.requested_transcript_version_id =
            TranscriptVersionId::from_parts(9, 3)
    );
    changed!(
        |r: &mut PlannedChapterModelRequest| r.requested_transcript_content_digest =
            ContentDigest::from_bytes([3; 32])
    );
    changed!(
        |r: &mut PlannedChapterModelRequest| r.selected_transcript_version_id =
            TranscriptVersionId::from_parts(9, 4)
    );
    changed!(
        |r: &mut PlannedChapterModelRequest| r.selected_transcript_content_digest =
            ContentDigest::from_bytes([4; 32])
    );
    changed!(|r: &mut PlannedChapterModelRequest| r.policy_version += 1);
    changed!(|r: &mut PlannedChapterModelRequest| r.provider.push('2'));
    changed!(|r: &mut PlannedChapterModelRequest| r.model.push('2'));
    changed!(|r: &mut PlannedChapterModelRequest| r.system_prompt.push('2'));
    changed!(|r: &mut PlannedChapterModelRequest| r.user_prompt.push('2'));
    changed!(|r: &mut PlannedChapterModelRequest| r.response_format =
        ChapterModelResponseFormat::Unsupported { wire_code: 7 });
    changed!(|r: &mut PlannedChapterModelRequest| r.maximum_completion_bytes += 1);
    changed!(|r: &mut PlannedChapterModelRequest| r.duration_milliseconds = None);
    changed!(
        |r: &mut PlannedChapterModelRequest| r.mode = ChapterModelObservationMode::Enrich {
            publisher_artifact: publisher_artifact()
        }
    );
    changed!(
        |r: &mut PlannedChapterModelRequest| r.expected_artifact_source =
            ChapterArtifactSource::AgentComposed
    );
    changed!(
        |r: &mut PlannedChapterModelRequest| r.expected_chapter_selection_revision =
            StateRevision::new(42)
    );
    assert_ne!(
        chapter_model_request_fingerprint(&baseline, "different/model").unwrap(),
        expected
    );
}

#[test]
fn enrichment_fingerprint_covers_base_identity_and_integrity() {
    let mut first = generated_request();
    first.mode = ChapterModelObservationMode::Enrich {
        publisher_artifact: publisher_artifact(),
    };
    let mut second = first.clone();
    let ChapterModelObservationMode::Enrich { publisher_artifact } = &mut second.mode else {
        unreachable!()
    };
    publisher_artifact.chapters[0].title.push_str(" revised");
    assert_ne!(fingerprint(&first), fingerprint(&second));

    let mut invalid = first;
    let ChapterModelObservationMode::Enrich { publisher_artifact } = &mut invalid.mode else {
        unreachable!()
    };
    publisher_artifact.source_revision.clear();
    assert_eq!(
        chapter_model_request_fingerprint(&invalid, "openai/gpt-4o-mini"),
        Err(ChapterModelRequestFingerprintError::InvalidEnrichmentBase)
    );
}

#[test]
fn raw_failure_evidence_classifies_retry_and_submission_risk() {
    use ChapterModelFailureEvidence as E;
    use ChapterModelRetryDisposition as R;
    use ModelChapterWorkflowFailureCode as C;

    let cases = [
        (
            E::MissingCredential,
            C::MissingCredential,
            R::ExplicitOnly,
            false,
            true,
        ),
        (
            E::HttpResponse { status_code: 429 },
            C::RateLimited,
            R::AutomaticRequest,
            false,
            true,
        ),
        (
            E::HttpResponse { status_code: 503 },
            C::ProviderUnavailable,
            R::ExplicitOnly,
            true,
            false,
        ),
        (
            E::Offline {
                submission_authorized: false,
            },
            C::Offline,
            R::AutomaticRequest,
            false,
            true,
        ),
        (
            E::TimedOut {
                submission_authorized: true,
            },
            C::AmbiguousSubmission,
            R::ExplicitOnly,
            true,
            false,
        ),
        (
            E::ResponseTooLarge,
            C::ResponseTooLarge,
            R::ExplicitOnly,
            true,
            false,
        ),
        (
            E::StalePublisherBase,
            C::StalePublisherBase,
            R::Replan,
            true,
            false,
        ),
        (
            E::StorageUnavailable {
                submission_authorized: true,
            },
            C::StorageUnavailable,
            R::ResumePersisted,
            true,
            false,
        ),
        (
            E::RetryExhausted {
                may_have_submitted: true,
            },
            C::RetryExhausted,
            R::Never,
            true,
            false,
        ),
        (
            E::Cancelled {
                submission_authorized: false,
            },
            C::Cancelled,
            R::Never,
            false,
            true,
        ),
    ];
    for (evidence, code, retry, may_have_submitted, resubmission_is_safe) in cases {
        assert_eq!(
            classify_chapter_model_failure(evidence),
            ChapterModelFailureClassification {
                code,
                retry,
                may_have_submitted,
                resubmission_is_safe,
            }
        );
    }
}

#[test]
fn workflow_actions_are_stage_and_failure_aware() {
    let terminal = ChapterModelFailureClassification {
        code: ModelChapterWorkflowFailureCode::InvalidRequest,
        retry: ChapterModelRetryDisposition::Never,
        may_have_submitted: false,
        resubmission_is_safe: true,
    };
    assert_eq!(
        model_chapter_allowed_actions(ModelChapterWorkflowStage::Requested, None),
        MODEL_CHAPTER_CANCEL_ACTION
    );
    assert_eq!(
        model_chapter_allowed_actions(ModelChapterWorkflowStage::Ambiguous, None),
        MODEL_CHAPTER_RETRY_CANCEL_ACTIONS
    );
    assert_eq!(
        model_chapter_allowed_actions(ModelChapterWorkflowStage::Blocked, Some(terminal)),
        MODEL_CHAPTER_CANCEL_ACTION
    );
    assert_eq!(
        model_chapter_allowed_actions(ModelChapterWorkflowStage::Succeeded, None),
        MODEL_CHAPTER_NO_ACTIONS
    );
}
