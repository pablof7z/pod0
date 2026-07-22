use pod0_domain::{
    ContentDigest, GeneratedArtifactId, HostRequestId, ScheduledAttemptId, ScheduledOccurrenceId,
    ScheduledTaskId, StateRevision, UnixTimestampMilliseconds,
};
use sha2::{Digest as _, Sha256};

pub const SCHEDULED_AGENT_POLICY_VERSION: u32 = 2;
pub const MAX_SCHEDULED_AGENT_TASKS: u16 = 200;
pub const MAX_SCHEDULED_AGENT_LABEL_BYTES: usize = 160;
pub const MAX_SCHEDULED_AGENT_PROMPT_BYTES: usize = 32 * 1_024;
pub const MAX_SCHEDULED_AGENT_MODEL_BYTES: usize = 256;
pub const MAX_SCHEDULED_AGENT_CONTEXT_MESSAGES: usize = 32;
pub const MAX_SCHEDULED_AGENT_CONTEXT_MESSAGE_BYTES: usize = 16 * 1_024;
pub const MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES: usize = 16 * 1_024;
pub const MAX_SCHEDULED_AGENT_SAFE_DETAIL_BYTES: usize = 1_024;
pub const MAX_SCHEDULED_AGENT_PROVIDER_OPERATION_BYTES: usize = 1_024;
pub const MAX_SCHEDULED_AGENT_ATTEMPTS: u16 = 12;
pub const SCHEDULED_AGENT_RETRY_DELAY_MILLISECONDS: i64 = 5 * 60 * 1_000;
pub const SCHEDULED_AGENT_HOST_DEADLINE_MILLISECONDS: i64 = 15 * 60 * 1_000;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ScheduledTaskInput {
    pub task_id: Option<ScheduledTaskId>,
    pub label: String,
    pub prompt: String,
    pub model_reference: String,
    pub interval_milliseconds: u64,
    pub next_run_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledTaskDefinition {
    pub task_id: ScheduledTaskId,
    pub label: String,
    pub prompt: String,
    pub prompt_revision: ContentDigest,
    pub model_reference: String,
    pub interval_milliseconds: u64,
    pub created_at: UnixTimestampMilliseconds,
    pub last_run_at: Option<UnixTimestampMilliseconds>,
    pub next_run_at: UnixTimestampMilliseconds,
    pub revision: StateRevision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ScheduledAgentStage {
    Pending,
    Requested,
    HostAccepted,
    RetryScheduled,
    Blocked,
    Cancelled,
    Obsolete,
    FailedPermanent,
    Succeeded,
    Ambiguous,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ScheduledAgentFailureCode {
    MissingCredential,
    Offline,
    Network,
    RateLimited,
    ProviderUnavailable,
    PermissionDenied,
    InvalidOutput,
    UnsafeToRetry,
    Cancelled,
    Unexpected,
    RetryExhausted,
    StorageUnavailable,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ScheduledAgentFailure {
    pub code: ScheduledAgentFailureCode,
    pub safe_detail: Option<String>,
    pub retryable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ScheduledAgentAllowedActions {
    pub can_retry: bool,
    pub can_cancel: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ScheduledTaskProjection {
    pub task_id: ScheduledTaskId,
    pub label: String,
    pub prompt: String,
    pub prompt_revision: ContentDigest,
    pub model_reference: String,
    pub interval_milliseconds: u64,
    pub last_run_at: Option<UnixTimestampMilliseconds>,
    pub next_run_at: UnixTimestampMilliseconds,
    pub task_revision: StateRevision,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ScheduledAgentWorkflowProjection {
    pub task_id: ScheduledTaskId,
    pub occurrence_id: ScheduledOccurrenceId,
    pub prompt_revision: ContentDigest,
    pub stage: ScheduledAgentStage,
    pub workflow_revision: StateRevision,
    pub attempt: u16,
    pub attempt_id: Option<ScheduledAttemptId>,
    pub request_id: Option<HostRequestId>,
    pub not_before: Option<UnixTimestampMilliseconds>,
    pub artifact_id: Option<GeneratedArtifactId>,
    pub output_digest: Option<ContentDigest>,
    pub failure: Option<ScheduledAgentFailure>,
    pub updated_at: UnixTimestampMilliseconds,
    pub allowed_actions: ScheduledAgentAllowedActions,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ScheduledAgentProjection {
    pub tasks: Vec<ScheduledTaskProjection>,
    pub workflows: Vec<ScheduledAgentWorkflowProjection>,
    pub has_more: bool,
    pub failure: Option<crate::CoreFailure>,
}

impl ScheduledAgentProjection {
    pub fn enforce_bounds(&mut self, offset: usize, requested_items: usize) {
        let limit = requested_items.clamp(
            1,
            usize::from(MAX_SCHEDULED_AGENT_TASKS.min(crate::MAX_PROJECTION_ITEMS)),
        );
        let task_count = self.tasks.len();
        let workflow_count = self.workflows.len();
        self.tasks = self.tasks.drain(..).skip(offset).take(limit).collect();
        self.workflows = self.workflows.drain(..).skip(offset).take(limit).collect();
        self.has_more |= task_count > offset.saturating_add(self.tasks.len())
            || workflow_count > offset.saturating_add(self.workflows.len());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ScheduledAgentContextRole {
    System,
    User,
    Assistant,
    Tool,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ScheduledAgentContextMessage {
    pub role: ScheduledAgentContextRole,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ScheduledAgentExecutionRequest {
    pub occurrence_id: ScheduledOccurrenceId,
    pub attempt_id: ScheduledAttemptId,
    pub prompt_revision: ContentDigest,
    pub prompt: String,
    pub model_reference: String,
    pub context: Vec<ScheduledAgentContextMessage>,
    pub maximum_output_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ScheduledAgentExecutionObservation {
    Accepted {
        occurrence_id: ScheduledOccurrenceId,
        attempt_id: ScheduledAttemptId,
        provider_operation_id: Option<String>,
    },
    Completed {
        occurrence_id: ScheduledOccurrenceId,
        attempt_id: ScheduledAttemptId,
        artifact_id: GeneratedArtifactId,
        output_digest: ContentDigest,
        output_excerpt: String,
    },
    Failed {
        occurrence_id: ScheduledOccurrenceId,
        attempt_id: ScheduledAttemptId,
        code: ScheduledAgentFailureCode,
        safe_detail: Option<String>,
        retry_after_milliseconds: Option<u64>,
    },
    Cancelled {
        occurrence_id: ScheduledOccurrenceId,
        attempt_id: ScheduledAttemptId,
    },
    Unsupported {
        wire_code: u32,
    },
}

#[must_use]
pub fn scheduled_occurrence_id(
    task_id: ScheduledTaskId,
    scheduled_for: UnixTimestampMilliseconds,
) -> ScheduledOccurrenceId {
    let mut hash = StableHash::new(b"pod0-scheduled-occurrence-v1");
    hash.bytes(&task_id.into_bytes());
    hash.i64(scheduled_for.value());
    ScheduledOccurrenceId::from_bytes(hash.first_16())
}

#[must_use]
pub fn scheduled_attempt_id(
    occurrence_id: ScheduledOccurrenceId,
    attempt: u16,
) -> Option<ScheduledAttemptId> {
    if attempt == 0 {
        return None;
    }
    let mut hash = StableHash::new(b"pod0-scheduled-attempt-v1");
    hash.bytes(&occurrence_id.into_bytes());
    hash.u64(u64::from(attempt));
    Some(ScheduledAttemptId::from_bytes(hash.first_16()))
}

#[must_use]
pub fn scheduled_host_request_id(attempt_id: ScheduledAttemptId) -> HostRequestId {
    let mut hash = StableHash::new(b"pod0-scheduled-host-request-v1");
    hash.bytes(&attempt_id.into_bytes());
    HostRequestId::from_bytes(hash.first_16())
}

#[must_use]
pub fn scheduled_prompt_revision(prompt: &str) -> Option<ContentDigest> {
    if prompt.trim().is_empty() || prompt.len() > MAX_SCHEDULED_AGENT_PROMPT_BYTES {
        return None;
    }
    let mut hash = StableHash::new(b"pod0-scheduled-prompt-v1");
    hash.bytes(prompt.as_bytes());
    Some(ContentDigest::from_bytes(hash.finish()))
}

pub(crate) struct StableHash(Sha256);

impl StableHash {
    pub(crate) fn new(domain: &[u8]) -> Self {
        let mut value = Self(Sha256::new());
        value.bytes(domain);
        value
    }

    pub(crate) fn bytes(&mut self, value: &[u8]) {
        self.0.update((value.len() as u64).to_be_bytes());
        self.0.update(value);
    }

    fn i64(&mut self, value: i64) {
        self.bytes(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_be_bytes());
    }

    pub(crate) fn first_16(self) -> [u8; 16] {
        self.finish()[..16].try_into().expect("digest prefix")
    }

    fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}
