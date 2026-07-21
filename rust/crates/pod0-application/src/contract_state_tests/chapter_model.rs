use super::*;

fn model_request(maximum_completion_bytes: u64) -> HostRequestEnvelope {
    HostRequestEnvelope {
        request_id: HostRequestId::from_parts(0, 8),
        command_id: CommandId::from_parts(0, 3),
        cancellation_id: CancellationId::from_parts(0, 4),
        issued_revision: StateRevision::new(5),
        deadline_at: None,
        request: HostRequest::ExecuteChapterModel {
            episode_id: EpisodeId::from_parts(2, 3),
            generation: 7,
            submission_fence_id: ChapterModelSubmissionFenceId::from_parts(8, 9),
            execution: crate::ChapterModelExecutionRequest {
                provider: "openrouter".into(),
                model: "model".into(),
                system_prompt: "system".into(),
                user_prompt: "user".into(),
                response_format: crate::ChapterModelResponseFormat::JsonObject,
                maximum_completion_bytes,
            },
        },
    }
}

fn model_observation(
    observation: HostObservation,
    sequence_number: u64,
) -> HostObservationEnvelope {
    HostObservationEnvelope {
        request_id: HostRequestId::from_parts(0, 8),
        cancellation_id: CancellationId::from_parts(0, 4),
        observed_request_revision: StateRevision::new(5),
        sequence_number,
        observed_at: UnixTimestampMilliseconds::new(2_000),
        observation,
    }
}

#[test]
fn chapter_model_provider_ack_keeps_stream_open_until_bounded_completion() {
    let mut ledger = HostRequestLedger::default();
    assert!(ledger.register(model_request(4)));
    let accepted = model_observation(
        HostObservation::ChapterModelProviderAccepted {
            episode_id: EpisodeId::from_parts(2, 3),
            generation: 7,
            submission_fence_id: ChapterModelSubmissionFenceId::from_parts(8, 9),
            update: crate::ChapterModelProviderUpdate {
                provider_operation_id: "operation-1".into(),
                provider_status: Some("running".into()),
            },
        },
        1,
    );
    assert_eq!(
        ledger.accept_observation(&accepted),
        ObservationAcceptance::Accepted
    );

    let oversized = model_observation(
        HostObservation::ChapterModelCompleted {
            episode_id: EpisodeId::from_parts(2, 3),
            generation: 7,
            submission_fence_id: ChapterModelSubmissionFenceId::from_parts(8, 9),
            completion: crate::ChapterModelCompletionObservation {
                completion: "12345".into(),
                provider: "openrouter".into(),
                model: "model".into(),
                prompt_tokens: None,
                completion_tokens: None,
                cached_tokens: None,
                reasoning_tokens: None,
                cost_microusd: None,
                provider_operation_id: Some("operation-1".into()),
                provider_status: Some("completed".into()),
                provider_generated_at: Some(UnixTimestampMilliseconds::new(1_900)),
            },
        },
        2,
    );
    assert_eq!(
        ledger.accept_observation(&oversized),
        ObservationAcceptance::PayloadTooLarge
    );

    let mut completed = oversized;
    if let HostObservation::ChapterModelCompleted { completion, .. } = &mut completed.observation {
        completion.completion = "1234".into();
    }
    assert_eq!(
        ledger.accept_observation(&completed),
        ObservationAcceptance::Accepted
    );
    assert_eq!(
        ledger.accept_observation(&completed),
        ObservationAcceptance::Duplicate
    );
}

#[test]
fn chapter_model_observations_require_exact_generation_and_fence() {
    let mut ledger = HostRequestLedger::default();
    assert!(ledger.register(model_request(32)));
    let failed = model_observation(
        HostObservation::ChapterModelFailed {
            episode_id: EpisodeId::from_parts(2, 3),
            generation: 8,
            submission_fence_id: ChapterModelSubmissionFenceId::from_parts(8, 9),
            code: crate::ChapterModelHostFailureCode::InvalidResponse,
            safe_detail: None,
            retry_after_milliseconds: None,
        },
        1,
    );
    assert_eq!(
        ledger.accept_observation(&failed),
        ObservationAcceptance::MismatchedPayload
    );
}
