use pod0_application::{
    ActivityFailureCode, PUBLISHER_CHAPTER_REQUEST_DEADLINE_MILLISECONDS,
    publisher_chapter_retry_delay_milliseconds,
};
use pod0_storage::{
    PublisherChapterObservationAction, PublisherChapterWorkflowFailureInput,
    PublisherChapterWorkflowRecord,
};

pub(super) fn failure_action(
    record: &PublisherChapterWorkflowRecord,
    failure_code: &str,
    retryable: bool,
    now_ms: i64,
    retry_issued_revision: pod0_domain::StateRevision,
) -> PublisherChapterObservationAction {
    let retry_at_ms = retryable
        .then(|| now_ms.checked_add(publisher_chapter_retry_delay_milliseconds(record.attempt)))
        .flatten();
    PublisherChapterObservationAction::Fail {
        failure: PublisherChapterWorkflowFailureInput {
            request_id: record.request_id.expect("active publisher request"),
            failure_code: failure_code.to_owned(),
            failure_detail: None,
            retry_at_ms,
            retry_issued_revision,
            retry_deadline_at_ms: retry_at_ms.and_then(|value| {
                value.checked_add(PUBLISHER_CHAPTER_REQUEST_DEADLINE_MILLISECONDS)
            }),
            observed_at_ms: now_ms,
        },
        outcome_code: activity_failure_code(failure_code),
    }
}

fn activity_failure_code(code: &str) -> ActivityFailureCode {
    match code {
        "offline" => ActivityFailureCode::Offline,
        "timed_out" => ActivityFailureCode::TimedOut,
        "response_too_large" => ActivityFailureCode::ResponseTooLarge,
        "not_found" | "invalid_response" | "invalid_document" | "selection_changed" => {
            ActivityFailureCode::InvalidResponse
        }
        _ => ActivityFailureCode::ProviderUnavailable,
    }
}
