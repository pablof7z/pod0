use super::*;

#[must_use]
pub fn classify_chapter_model_failure(
    evidence: ChapterModelFailureEvidence,
    phase: ChapterModelFailurePhase,
) -> ChapterModelFailureClassification {
    use ChapterModelFailureEvidence as E;
    use ChapterModelRetryDisposition as R;
    use ModelChapterWorkflowFailureCode as C;
    let classified = match evidence {
        E::MissingCredential => (C::MissingCredential, R::ExplicitOnly, false, true),
        E::InvalidRequest | E::UnsupportedProvider => (C::InvalidRequest, R::Never, false, true),
        E::CoreUnavailable => (C::StorageUnavailable, R::AutomaticRequest, false, true),
        E::HttpResponse { status_code } => classify_http(status_code),
        E::Offline if phase.submission_authorized => {
            (C::AmbiguousSubmission, R::ExplicitOnly, true, false)
        }
        E::TimedOut if phase.submission_authorized => {
            (C::AmbiguousSubmission, R::ExplicitOnly, true, false)
        }
        E::Transport if phase.submission_authorized => {
            (C::AmbiguousSubmission, R::ExplicitOnly, true, false)
        }
        E::Offline => (C::Offline, R::AutomaticRequest, false, true),
        E::TimedOut => (C::TimedOut, R::AutomaticRequest, false, true),
        E::Transport => (C::Transport, R::AutomaticRequest, false, true),
        E::ResponseTooLarge => (C::ResponseTooLarge, R::ExplicitOnly, true, false),
        E::InvalidResponse => (C::InvalidResponse, R::ExplicitOnly, true, false),
        E::Qualification { .. } => (C::QualificationRejected, R::ExplicitOnly, true, false),
        E::StaleTranscript => (C::StaleTranscript, R::Replan, true, false),
        E::StalePublisherBase => (C::StalePublisherBase, R::Replan, true, false),
        E::SelectionChanged => (C::SelectionChanged, R::Replan, true, false),
        E::StorageUnavailable => (
            C::StorageUnavailable,
            R::ResumePersisted,
            phase.submission_authorized,
            !phase.submission_authorized,
        ),
        E::ProviderRecoveryUnavailable => {
            (C::ProviderRecoveryUnavailable, R::ExplicitOnly, true, false)
        }
        E::RetryExhausted => (
            C::RetryExhausted,
            R::Never,
            phase.submission_authorized,
            false,
        ),
        E::Cancelled if phase.submission_authorized => {
            (C::AmbiguousSubmission, R::ExplicitOnly, true, false)
        }
        E::Cancelled => (C::Cancelled, R::Never, false, true),
        E::Unsupported { wire_code } => (C::Unsupported { wire_code }, R::Never, false, true),
    };
    ChapterModelFailureClassification {
        code: classified.0,
        retry: classified.1,
        may_have_submitted: classified.2,
        resubmission_is_safe: classified.3,
    }
}

fn classify_http(
    status: u16,
) -> (
    ModelChapterWorkflowFailureCode,
    ChapterModelRetryDisposition,
    bool,
    bool,
) {
    use ChapterModelRetryDisposition as R;
    use ModelChapterWorkflowFailureCode as C;
    match status {
        401 | 403 => (C::MissingCredential, R::ExplicitOnly, false, true),
        408 => (C::TimedOut, R::ExplicitOnly, true, false),
        429 => (C::RateLimited, R::AutomaticRequest, false, true),
        500..=599 => (C::ProviderUnavailable, R::ExplicitOnly, true, false),
        400..=499 => (C::ProviderRejected, R::Never, false, true),
        _ => (C::InvalidResponse, R::ExplicitOnly, true, false),
    }
}

#[must_use]
pub fn model_chapter_allowed_actions(
    stage: ModelChapterWorkflowStage,
    failure: Option<ChapterModelFailureClassification>,
) -> ModelChapterWorkflowAllowedActions {
    use ModelChapterWorkflowStage as S;
    match stage {
        S::Preserved
        | S::SubmissionAuthorized
        | S::ProviderAccepted
        | S::CompletionObserved
        | S::Succeeded
        | S::Unsupported { .. } => MODEL_CHAPTER_NO_ACTIONS,
        S::Failed => match failure {
            Some(value) if value.retry == ChapterModelRetryDisposition::Never => {
                MODEL_CHAPTER_NO_ACTIONS
            }
            _ => MODEL_CHAPTER_RETRY_ACTION,
        },
        S::Cancelled => MODEL_CHAPTER_RETRY_ACTION,
        S::Ambiguous | S::Blocked => match failure {
            Some(value) if value.retry == ChapterModelRetryDisposition::Never => {
                MODEL_CHAPTER_CANCEL_ACTION
            }
            _ => MODEL_CHAPTER_RETRY_CANCEL_ACTIONS,
        },
        S::AwaitingTranscript | S::AwaitingPublisher | S::Requested | S::RetryScheduled => {
            MODEL_CHAPTER_CANCEL_ACTION
        }
    }
}
