use pod0_domain::{
    CancellationId, ChapterModelSubmissionFenceId, ContentDigest, EpisodeId, HostRequestId,
    StateRevision,
};

use super::model::ModelChapterWorkflowRecord;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChapterCompletionRecord {
    pub request_id: HostRequestId,
    pub episode_id: EpisodeId,
    pub generation: u64,
    pub submission_fence_id: ChapterModelSubmissionFenceId,
    pub completion: String,
    pub completion_digest: ContentDigest,
    pub provider: String,
    pub model: String,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub cost_microusd: Option<u64>,
    pub provider_operation_id: Option<String>,
    pub provider_status: Option<String>,
    pub generated_at_ms: i64,
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelChapterSubmissionClaim {
    Authorized(ModelChapterWorkflowRecord),
    AlreadyClaimed(ModelChapterWorkflowRecord),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChapterSubmissionClaimInput {
    pub episode_id: EpisodeId,
    pub request_id: HostRequestId,
    pub generation: u64,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub now_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChapterProviderAcceptedInput {
    pub episode_id: EpisodeId,
    pub request_id: HostRequestId,
    pub generation: u64,
    pub submission_fence_id: ChapterModelSubmissionFenceId,
    pub provider_operation_id: String,
    pub provider_status: Option<String>,
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChapterCompletionInput {
    pub episode_id: EpisodeId,
    pub request_id: HostRequestId,
    pub generation: u64,
    pub submission_fence_id: ChapterModelSubmissionFenceId,
    pub completion: String,
    pub provider: String,
    pub model: String,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub cost_microusd: Option<u64>,
    pub provider_operation_id: Option<String>,
    pub provider_status: Option<String>,
    pub generated_at_ms: i64,
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelChapterFailureDisposition {
    Retry {
        not_before_ms: i64,
        deadline_at_ms: i64,
        issued_revision: StateRevision,
        evidence_permits_resubmission: bool,
    },
    Replan,
    Block,
    Fail,
    Ambiguous,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChapterFailureInput {
    pub episode_id: EpisodeId,
    pub request_id: HostRequestId,
    pub generation: u64,
    pub submission_fence_id: ChapterModelSubmissionFenceId,
    pub failure_code: String,
    pub failure_detail: Option<String>,
    pub may_have_submitted: bool,
    pub disposition: ModelChapterFailureDisposition,
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChapterRecoveryReport {
    pub ambiguous_requests: Vec<HostRequestId>,
    pub resumable_provider_requests: Vec<HostRequestId>,
    pub staged_completions: Vec<HostRequestId>,
    pub has_more: bool,
}
