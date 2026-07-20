use crate::ChapterObservationRejection;

mod failure;
mod fingerprint;

pub use failure::*;
pub use fingerprint::*;

pub const MODEL_CHAPTER_WORKFLOW_POLICY_VERSION: u32 = 1;
pub const MODEL_CHAPTER_WORKFLOW_MAX_ATTEMPTS: u16 = 8;
pub const MODEL_CHAPTER_REQUEST_DEADLINE_MILLISECONDS: i64 = 60_000;
pub const MAX_ACTIVE_MODEL_CHAPTER_REQUESTS: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ModelChapterWorkflowAllowedActions {
    pub can_retry: bool,
    pub can_cancel: bool,
}

pub const MODEL_CHAPTER_NO_ACTIONS: ModelChapterWorkflowAllowedActions = actions(false, false);
pub const MODEL_CHAPTER_CANCEL_ACTION: ModelChapterWorkflowAllowedActions = actions(false, true);
pub const MODEL_CHAPTER_RETRY_ACTION: ModelChapterWorkflowAllowedActions = actions(true, false);
pub const MODEL_CHAPTER_RETRY_CANCEL_ACTIONS: ModelChapterWorkflowAllowedActions =
    actions(true, true);

const fn actions(can_retry: bool, can_cancel: bool) -> ModelChapterWorkflowAllowedActions {
    ModelChapterWorkflowAllowedActions {
        can_retry,
        can_cancel,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ModelChapterWorkflowStage {
    AwaitingTranscript,
    AwaitingPublisher,
    Preserved,
    Requested,
    SubmissionAuthorized,
    ProviderAccepted,
    Ambiguous,
    CompletionObserved,
    RetryScheduled,
    Blocked,
    Failed,
    Cancelled,
    Succeeded,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ModelChapterWorkflowFailureCode {
    MissingCredential,
    InvalidRequest,
    RateLimited,
    ProviderRejected,
    ProviderUnavailable,
    Offline,
    TimedOut,
    Transport,
    ResponseTooLarge,
    InvalidResponse,
    QualificationRejected,
    StaleTranscript,
    StalePublisherBase,
    SelectionChanged,
    StorageUnavailable,
    AmbiguousSubmission,
    ProviderRecoveryUnavailable,
    RetryExhausted,
    Cancelled,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChapterModelFailureEvidence {
    MissingCredential,
    InvalidRequest,
    UnsupportedProvider,
    CoreUnavailable,
    HttpResponse { status_code: u16 },
    Offline { submission_authorized: bool },
    TimedOut { submission_authorized: bool },
    Transport { submission_authorized: bool },
    ResponseTooLarge,
    InvalidResponse,
    Qualification { reason: ChapterObservationRejection },
    StaleTranscript,
    StalePublisherBase,
    SelectionChanged,
    StorageUnavailable { submission_authorized: bool },
    ProviderRecoveryUnavailable,
    RetryExhausted { may_have_submitted: bool },
    Cancelled { submission_authorized: bool },
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChapterModelRetryDisposition {
    AutomaticRequest,
    Replan,
    ResumePersisted,
    ExplicitOnly,
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterModelFailureClassification {
    pub code: ModelChapterWorkflowFailureCode,
    pub retry: ChapterModelRetryDisposition,
    pub may_have_submitted: bool,
}
