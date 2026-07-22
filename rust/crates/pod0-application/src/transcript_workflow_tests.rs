use pod0_domain::{
    ContentDigest, EpisodeId, PodcastId, StateRevision, TranscriptAttemptId,
    TranscriptSubmissionFenceId, TranscriptVersionId, TranscriptWorkflowId,
    UnixTimestampMilliseconds,
};

use super::*;

fn input(origin: TranscriptWorkflowOrigin) -> TranscriptWorkflowPlanInput {
    TranscriptWorkflowPlanInput {
        episode_id: EpisodeId::from_bytes([1; 16]),
        source_revision: "audio-v1".to_owned(),
        committed_transcript: None,
        selected_evidence_input_version: None,
        origin,
        configured_provider: TranscriptProvider::AssemblyAi,
        configured_model: "universal-3-pro".to_owned(),
        remote_audio_url: "https://example.test/episode.mp3".to_owned(),
        local_audio_url: None,
        publisher_transcript_url: None,
        publisher_mime_hint: None,
        auto_publisher_enabled: false,
        auto_provider_enabled: true,
        credential_available: true,
        embedding_space_id: "embedding-space-v1".to_owned(),
    }
}

#[test]
fn user_request_is_deterministic_and_skips_publisher_fallback_first() {
    let mut value = input(TranscriptWorkflowOrigin::User);
    value.publisher_transcript_url = Some("https://example.test/transcript.vtt".to_owned());
    value.auto_publisher_enabled = true;
    let first = plan_transcript_workflow(value.clone());
    let second = plan_transcript_workflow(value);

    assert_eq!(first, second);
    assert_eq!(first.generation, TranscriptGenerationDecision::Ensure);
    let request = first.request.unwrap();
    assert!(!request.publisher_first);
    assert!(request.provider_fallback_enabled);
}

#[test]
fn automatic_request_uses_publisher_before_a_configured_provider() {
    let mut value = input(TranscriptWorkflowOrigin::Automatic);
    value.publisher_transcript_url = Some("https://example.test/transcript.json".to_owned());
    value.publisher_mime_hint = Some("application/json".to_owned());
    value.auto_publisher_enabled = true;
    value.credential_available = false;

    let plan = plan_transcript_workflow(value);
    assert_eq!(plan.generation, TranscriptGenerationDecision::Ensure);
    let request = plan.request.unwrap();
    assert!(request.publisher_first);
    assert!(request.provider_fallback_enabled);
}

#[test]
fn automatic_remote_work_is_not_created_without_a_credential() {
    let mut value = input(TranscriptWorkflowOrigin::Automatic);
    value.credential_available = false;

    let plan = plan_transcript_workflow(value);
    assert_eq!(plan.generation, TranscriptGenerationDecision::NotRequested);
    assert!(plan.request.is_none());
}

#[test]
fn explicit_remote_and_apple_requests_surface_missing_prerequisites() {
    let mut remote = input(TranscriptWorkflowOrigin::User);
    remote.credential_available = false;
    assert_eq!(
        plan_transcript_workflow(remote).generation,
        TranscriptGenerationDecision::AwaitingCredential {
            provider: TranscriptProvider::AssemblyAi,
        }
    );

    let mut apple = input(TranscriptWorkflowOrigin::User);
    apple.configured_provider = TranscriptProvider::AppleSpeech;
    apple.configured_model = "apple-speech-v1".to_owned();
    assert_eq!(
        plan_transcript_workflow(apple).generation,
        TranscriptGenerationDecision::AwaitingLocalAudio
    );
}

#[test]
fn committed_generation_drives_a_deterministic_evidence_version() {
    let mut value = input(TranscriptWorkflowOrigin::Automatic);
    let version = TranscriptVersionId::from_bytes([2; 16]);
    let digest = ContentDigest::from_bytes([3; 32]);
    value.committed_transcript = Some(CommittedTranscriptGeneration {
        source_revision: value.source_revision.clone(),
        transcript_version_id: version,
        content_digest: digest,
    });
    let expected =
        transcript_evidence_input_version(version, digest, &value.embedding_space_id).unwrap();

    let needs_index = plan_transcript_workflow(value.clone());
    assert_eq!(
        needs_index.generation,
        TranscriptGenerationDecision::Current
    );
    assert_eq!(
        needs_index.evidence,
        TranscriptEvidenceDecision::Ensure {
            input_version: expected.clone(),
        }
    );

    value.selected_evidence_input_version = Some(expected);
    assert_eq!(
        plan_transcript_workflow(value).evidence,
        TranscriptEvidenceDecision::Current
    );
}

#[test]
fn identities_are_stable_and_attempt_zero_is_rejected() {
    let workflow = transcript_workflow_id(
        EpisodeId::from_bytes([4; 16]),
        "audio-v1",
        TranscriptProvider::ElevenLabsScribe,
        "scribe-v2",
    );
    assert_eq!(
        workflow,
        transcript_workflow_id(
            EpisodeId::from_bytes([4; 16]),
            "audio-v1",
            TranscriptProvider::ElevenLabsScribe,
            "scribe-v2",
        )
    );
    assert!(transcript_attempt_id(workflow, 0).is_none());
    let attempt = transcript_attempt_id(workflow, 1).unwrap();
    assert_ne!(attempt, transcript_attempt_id(workflow, 2).unwrap());
    assert_eq!(
        transcript_submission_fence_id(attempt),
        transcript_submission_fence_id(attempt)
    );
}

#[test]
fn failure_classification_never_resubmits_an_ambiguous_attempt() {
    let before = classify_transcript_failure(TranscriptFailureEvidence::Transport {
        submission_authorized: false,
        provider_accepted: false,
    });
    assert_eq!(before.retry, TranscriptRetryDisposition::AutomaticRequest);
    assert!(before.resubmission_is_safe);

    let ambiguous = classify_transcript_failure(TranscriptFailureEvidence::Transport {
        submission_authorized: true,
        provider_accepted: false,
    });
    assert_eq!(ambiguous.retry, TranscriptRetryDisposition::ExplicitOnly);
    assert!(ambiguous.may_have_submitted);
    assert!(!ambiguous.resubmission_is_safe);

    let accepted = classify_transcript_failure(TranscriptFailureEvidence::TimedOut {
        submission_authorized: true,
        provider_accepted: true,
    });
    assert_eq!(accepted.retry, TranscriptRetryDisposition::RecoverPersisted);
    assert!(!accepted.resubmission_is_safe);
}

#[test]
fn retry_and_deadline_time_are_kernel_owned_and_bounded() {
    assert_eq!(transcript_retry_delay_milliseconds(1, None), 5_000);
    assert_eq!(transcript_retry_delay_milliseconds(2, None), 10_000);
    assert_eq!(transcript_retry_delay_milliseconds(99, None), 3_600_000);
    assert_eq!(
        transcript_retry_not_before(UnixTimestampMilliseconds::new(10_000), 2, Some(40_000)),
        UnixTimestampMilliseconds::new(50_000)
    );
    assert_eq!(
        transcript_host_request_deadline(UnixTimestampMilliseconds::new(10_000)),
        UnixTimestampMilliseconds::new(130_000)
    );
}

#[test]
fn capability_validation_rejects_unbounded_or_unsupported_requests() {
    let context = TranscriptCapabilityContext {
        episode_id: EpisodeId::from_bytes([1; 16]),
        podcast_id: PodcastId::from_bytes([2; 16]),
        source_revision: "audio-v1".to_owned(),
    };
    let request = TranscriptCapabilityRequest::FetchPublisher {
        context: context.clone(),
        source_url: "https://example.test/transcript.vtt".to_owned(),
        mime_hint: Some("text/vtt".to_owned()),
        maximum_response_bytes: MAX_TRANSCRIPT_CAPABILITY_RESPONSE_BYTES,
    };
    assert_eq!(
        validate_transcript_capability_request(request),
        TranscriptCapabilityValidation::Accepted
    );
    let unsupported = TranscriptCapabilityRequest::SubmitProvider {
        context: context.clone(),
        attempt_id: TranscriptAttemptId::from_bytes([5; 16]),
        submission_fence_id: TranscriptSubmissionFenceId::from_bytes([6; 16]),
        provider: TranscriptProvider::AppleSpeech,
        model: "apple".to_owned(),
        audio_url: "file:///tmp/audio.m4a".to_owned(),
        maximum_response_bytes: 1,
    };
    assert_eq!(
        validate_transcript_capability_request(unsupported),
        TranscriptCapabilityValidation::Rejected {
            code: TranscriptWorkflowFailureCode::UnsupportedProvider,
        }
    );

    let remote_local_audio = TranscriptCapabilityRequest::TranscribeLocal {
        context: context.clone(),
        attempt_id: TranscriptAttemptId::from_bytes([5; 16]),
        audio_url: "https://example.test/audio.m4a".to_owned(),
        locale: None,
    };
    assert_eq!(
        validate_transcript_capability_request(remote_local_audio),
        TranscriptCapabilityValidation::Rejected {
            code: TranscriptWorkflowFailureCode::InvalidRequest,
        }
    );

    let recovery = TranscriptCapabilityRequest::RecoverProvider {
        context,
        attempt_id: TranscriptAttemptId::from_bytes([5; 16]),
        submission_fence_id: TranscriptSubmissionFenceId::from_bytes([6; 16]),
        provider: TranscriptProvider::AssemblyAi,
        model: "universal-3-pro".to_owned(),
        external_operation_id: "operation-1".to_owned(),
        provider_status: Some("processing".to_owned()),
        maximum_response_bytes: 1,
    };
    assert_eq!(
        validate_transcript_capability_request(recovery),
        TranscriptCapabilityValidation::Accepted
    );
}

#[test]
fn workflow_projection_is_bounded() {
    let workflow_id = TranscriptWorkflowId::from_bytes([7; 16]);
    let mut page = TranscriptWorkflowsProjection {
        workflows: (0..205)
            .map(|index| TranscriptWorkflowProjection {
                episode_id: EpisodeId::from_parts(0, index),
                workflow_id,
                source_revision: "audio-v1".to_owned(),
                origin: TranscriptWorkflowOrigin::Automatic,
                provider: TranscriptProvider::AssemblyAi,
                model: "model".to_owned(),
                stage: TranscriptWorkflowStage::Requested,
                workflow_revision: StateRevision::new(index),
                attempt: 0,
                attempt_id: None,
                submission_fence_id: None,
                request_id: None,
                external_operation_present: false,
                not_before: None,
                failure: None,
                updated_at: UnixTimestampMilliseconds::new(index as i64),
                allowed_actions: transcript_allowed_actions(TranscriptWorkflowStage::Requested),
            })
            .collect(),
        has_more: false,
        failure: None,
    };
    page.enforce_bounds(2, u16::MAX as usize);
    assert_eq!(page.workflows.len(), 200);
    assert!(page.has_more);
}
