use crate::{
    TranscriptFailureClassification, TranscriptFailureEvidence, TranscriptRetryDisposition,
    TranscriptWorkflowAllowedActions, TranscriptWorkflowFailureCode, TranscriptWorkflowStage,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TranscriptFailurePhase {
    pub submission_authorized: bool,
    pub provider_accepted: bool,
}

#[must_use]
pub const fn transcript_allowed_actions(
    stage: TranscriptWorkflowStage,
) -> TranscriptWorkflowAllowedActions {
    use TranscriptWorkflowStage as Stage;
    match stage {
        Stage::AwaitingPrerequisite | Stage::RetryScheduled | Stage::Blocked => {
            TranscriptWorkflowAllowedActions {
                can_retry: true,
                can_cancel: true,
            }
        }
        Stage::Requested
        | Stage::PublisherRequested
        | Stage::SubmissionAuthorized
        | Stage::ProviderAccepted
        | Stage::CompletionObserved
        | Stage::TranscriptCommitted
        | Stage::EvidenceRequested => TranscriptWorkflowAllowedActions {
            can_retry: false,
            can_cancel: true,
        },
        Stage::Failed => TranscriptWorkflowAllowedActions {
            can_retry: true,
            can_cancel: false,
        },
        Stage::Cancelled | Stage::Succeeded | Stage::Unsupported { .. } => {
            TranscriptWorkflowAllowedActions {
                can_retry: false,
                can_cancel: false,
            }
        }
    }
}

#[must_use]
pub const fn classify_transcript_failure(
    evidence: TranscriptFailureEvidence,
    phase: TranscriptFailurePhase,
) -> TranscriptFailureClassification {
    use TranscriptFailureEvidence as Evidence;
    use TranscriptRetryDisposition as Retry;
    use TranscriptWorkflowFailureCode as Code;
    match evidence {
        Evidence::MissingCredential => {
            classification(Code::MissingCredential, Retry::Replan, false, true)
        }
        Evidence::MissingLocalAudio => {
            classification(Code::MissingLocalAudio, Retry::Replan, false, true)
        }
        Evidence::InvalidRequest => {
            classification(Code::InvalidRequest, Retry::Never, false, false)
        }
        Evidence::UnsupportedProvider => {
            classification(Code::UnsupportedProvider, Retry::Never, false, false)
        }
        Evidence::PublisherUnavailable => {
            classification(Code::PublisherUnavailable, Retry::Replan, false, true)
        }
        Evidence::Offline => phase_failure(Code::Offline, phase),
        Evidence::RateLimited => phase_failure(Code::RateLimited, phase),
        Evidence::TimedOut => phase_failure(Code::TimedOut, phase),
        Evidence::Transport => phase_failure(Code::Transport, phase),
        Evidence::PermissionDenied => {
            classification(Code::PermissionDenied, Retry::ExplicitOnly, false, true)
        }
        Evidence::ProviderRejected => {
            classification(Code::ProviderRejected, Retry::Never, true, false)
        }
        Evidence::ProviderUnavailable => phase_failure(Code::ProviderUnavailable, phase),
        Evidence::ResponseTooLarge => {
            classification(Code::ResponseTooLarge, Retry::Never, true, false)
        }
        Evidence::InvalidResponse => {
            classification(Code::InvalidResponse, Retry::ExplicitOnly, true, false)
        }
        Evidence::StaleInput => classification(Code::StaleInput, Retry::Replan, false, false),
        Evidence::StorageUnavailable => phase_failure(Code::StorageUnavailable, phase),
        Evidence::ProviderRecoveryUnavailable => classification(
            Code::ProviderRecoveryUnavailable,
            Retry::ExplicitOnly,
            true,
            false,
        ),
        Evidence::RetryExhausted => classification(
            Code::RetryExhausted,
            Retry::ExplicitOnly,
            phase.submission_authorized || phase.provider_accepted,
            !phase.submission_authorized && !phase.provider_accepted,
        ),
        Evidence::Cancelled => classification(
            Code::Cancelled,
            Retry::Never,
            phase.submission_authorized || phase.provider_accepted,
            false,
        ),
        Evidence::Unsupported { wire_code } => {
            classification(Code::Unsupported { wire_code }, Retry::Never, false, false)
        }
    }
}

const fn phase_failure(
    code: TranscriptWorkflowFailureCode,
    phase: TranscriptFailurePhase,
) -> TranscriptFailureClassification {
    if phase.provider_accepted {
        classification(
            code,
            TranscriptRetryDisposition::RecoverPersisted,
            true,
            false,
        )
    } else if phase.submission_authorized {
        classification(code, TranscriptRetryDisposition::ExplicitOnly, true, false)
    } else {
        classification(
            code,
            TranscriptRetryDisposition::AutomaticRequest,
            false,
            true,
        )
    }
}

const fn classification(
    code: TranscriptWorkflowFailureCode,
    retry: TranscriptRetryDisposition,
    may_have_submitted: bool,
    resubmission_is_safe: bool,
) -> TranscriptFailureClassification {
    TranscriptFailureClassification {
        code,
        retry,
        may_have_submitted,
        resubmission_is_safe,
    }
}
