use super::*;

#[must_use]
pub fn classify_chapter_model_failure(
    evidence: ChapterModelFailureEvidence,
) -> ChapterModelFailureClassification {
    use ChapterModelFailureEvidence as E;
    use ChapterModelRetryDisposition as R;
    use ModelChapterWorkflowFailureCode as C;
    let classified = match evidence {
        E::MissingCredential => (C::MissingCredential, R::ExplicitOnly, false),
        E::InvalidRequest | E::UnsupportedProvider => (C::InvalidRequest, R::Never, false),
        E::CoreUnavailable => (C::StorageUnavailable, R::AutomaticRequest, false),
        E::HttpResponse { status_code } => classify_http(status_code),
        E::Offline {
            submission_authorized: true,
        }
        | E::TimedOut {
            submission_authorized: true,
        }
        | E::Transport {
            submission_authorized: true,
        } => (C::AmbiguousSubmission, R::ExplicitOnly, true),
        E::Offline {
            submission_authorized: false,
        } => (C::Offline, R::AutomaticRequest, false),
        E::TimedOut {
            submission_authorized: false,
        } => (C::TimedOut, R::AutomaticRequest, false),
        E::Transport {
            submission_authorized: false,
        } => (C::Transport, R::AutomaticRequest, false),
        E::ResponseTooLarge => (C::ResponseTooLarge, R::ExplicitOnly, true),
        E::InvalidResponse => (C::InvalidResponse, R::ExplicitOnly, true),
        E::Qualification { .. } => (C::QualificationRejected, R::ExplicitOnly, true),
        E::StaleTranscript => (C::StaleTranscript, R::Replan, true),
        E::StalePublisherBase => (C::StalePublisherBase, R::Replan, true),
        E::SelectionChanged => (C::SelectionChanged, R::Replan, true),
        E::StorageUnavailable {
            submission_authorized,
        } => (
            C::StorageUnavailable,
            R::ResumePersisted,
            submission_authorized,
        ),
        E::ProviderRecoveryUnavailable => (C::ProviderRecoveryUnavailable, R::ExplicitOnly, true),
        E::RetryExhausted { may_have_submitted } => {
            (C::RetryExhausted, R::Never, may_have_submitted)
        }
        E::Cancelled {
            submission_authorized: true,
        } => (C::AmbiguousSubmission, R::ExplicitOnly, true),
        E::Cancelled {
            submission_authorized: false,
        } => (C::Cancelled, R::Never, false),
        E::Unsupported { wire_code } => (C::Unsupported { wire_code }, R::Never, false),
    };
    ChapterModelFailureClassification {
        code: classified.0,
        retry: classified.1,
        may_have_submitted: classified.2,
    }
}

fn classify_http(
    status: u16,
) -> (
    ModelChapterWorkflowFailureCode,
    ChapterModelRetryDisposition,
    bool,
) {
    use ChapterModelRetryDisposition as R;
    use ModelChapterWorkflowFailureCode as C;
    match status {
        401 | 403 => (C::MissingCredential, R::ExplicitOnly, true),
        408 => (C::TimedOut, R::AutomaticRequest, true),
        429 => (C::RateLimited, R::AutomaticRequest, true),
        500..=599 => (C::ProviderUnavailable, R::AutomaticRequest, true),
        400..=499 => (C::ProviderRejected, R::Never, true),
        _ => (C::InvalidResponse, R::ExplicitOnly, true),
    }
}

#[must_use]
pub fn model_chapter_allowed_actions(
    stage: ModelChapterWorkflowStage,
    failure: Option<ChapterModelFailureClassification>,
) -> ModelChapterWorkflowAllowedActions {
    use ModelChapterWorkflowStage as S;
    match stage {
        S::Preserved | S::Succeeded | S::Unsupported { .. } => MODEL_CHAPTER_NO_ACTIONS,
        S::Failed | S::Cancelled => match failure {
            Some(value) if value.retry == ChapterModelRetryDisposition::Never => {
                MODEL_CHAPTER_NO_ACTIONS
            }
            _ => MODEL_CHAPTER_RETRY_ACTION,
        },
        S::Ambiguous | S::Blocked => match failure {
            Some(value) if value.retry == ChapterModelRetryDisposition::Never => {
                MODEL_CHAPTER_CANCEL_ACTION
            }
            _ => MODEL_CHAPTER_RETRY_CANCEL_ACTIONS,
        },
        _ => MODEL_CHAPTER_CANCEL_ACTION,
    }
}
