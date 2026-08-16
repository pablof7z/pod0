use super::*;

pub(super) fn model_completion(
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
