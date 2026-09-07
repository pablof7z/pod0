use pod0_domain::*;

use crate::*;

fn durable(execution: DurableEffectExecution) -> DurableExternalEffectRequest {
    DurableExternalEffectRequest {
        kind: ExternalEffectKind::TranscriptProvider,
        subject: ActivitySubject::Operation {
            command_id: CommandId::from_parts(1, 1),
        },
        episode_id: None,
        not_before: None,
        deadline_at: None,
        execution,
    }
}

fn transcript(capability: TranscriptCapabilityRequest) -> DurableEffectExecution {
    DurableEffectExecution::Transcript {
        request: DurableTranscriptEffectRequest {
            request_id: HostRequestId::from_parts(1, 2),
            command_id: CommandId::from_parts(1, 1),
            cancellation_id: CancellationId::from_parts(1, 3),
            issued_revision: StateRevision::INITIAL,
            deadline_at: None,
            capability,
        },
    }
}

fn transcript_context() -> TranscriptCapabilityContext {
    TranscriptCapabilityContext {
        episode_id: EpisodeId::from_parts(2, 1),
        podcast_id: PodcastId::from_parts(2, 2),
        source_revision: "source".into(),
    }
}

#[test]
fn only_exact_external_operation_recovery_may_reclaim_an_expired_submission() {
    let submit = durable(transcript(TranscriptCapabilityRequest::SubmitProvider {
        context: transcript_context(),
        attempt_id: TranscriptAttemptId::from_parts(3, 1),
        submission_fence_id: TranscriptSubmissionFenceId::from_parts(3, 2),
        provider: TranscriptProvider::AssemblyAi,
        model: "universal-2".into(),
        audio_url: "file:///audio.mp3".into(),
        maximum_response_bytes: 1_024,
    }));
    assert!(!submit.expired_lease_reclaim_is_exact());

    let recover = durable(transcript(TranscriptCapabilityRequest::RecoverProvider {
        context: transcript_context(),
        attempt_id: TranscriptAttemptId::from_parts(3, 1),
        submission_fence_id: TranscriptSubmissionFenceId::from_parts(3, 2),
        provider: TranscriptProvider::AssemblyAi,
        model: "universal-2".into(),
        external_operation_id: "provider-operation-1".into(),
        provider_status: Some("queued".into()),
        maximum_response_bytes: 1_024,
    }));
    assert!(recover.expired_lease_reclaim_is_exact());
}
