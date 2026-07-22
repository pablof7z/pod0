use pod0_application::ScheduledAgentFailureCode;
use pod0_domain::{ContentDigest, ScheduledTaskId, UnixTimestampMilliseconds};
use pod0_storage::{LegacyScheduledAgentCutoverReport, ScheduledAgentCutoverState, StorageError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyScheduledAgentCutoverStage {
    NotStarted,
    Staged,
    Verified,
    Authoritative,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyScheduledAgentCutoverFailureCode {
    InvalidSource,
    ConflictingCoreState,
    StorageUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyScheduledAgentCutoverFailure {
    pub code: LegacyScheduledAgentCutoverFailureCode,
    pub diagnostic_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyScheduledAgentTaskInput {
    pub task_id: ScheduledTaskId,
    pub label: String,
    pub prompt: String,
    pub model_reference: String,
    pub interval_milliseconds: u64,
    pub created_at: UnixTimestampMilliseconds,
    pub last_run_at: Option<UnixTimestampMilliseconds>,
    pub next_run_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyScheduledAgentOccurrenceDisposition {
    Pending,
    RetryScheduled {
        attempt: u16,
        not_before: UnixTimestampMilliseconds,
        failure_code: ScheduledAgentFailureCode,
        safe_detail: Option<String>,
    },
    Blocked {
        attempt: u16,
        failure_code: ScheduledAgentFailureCode,
        safe_detail: Option<String>,
        retryable: bool,
    },
    Ambiguous {
        attempt: u16,
        safe_detail: Option<String>,
    },
    FailedPermanent {
        attempt: u16,
        failure_code: ScheduledAgentFailureCode,
        safe_detail: Option<String>,
    },
    Cancelled {
        attempt: u16,
    },
    Obsolete {
        attempt: u16,
    },
    Succeeded {
        attempt: u16,
        output_excerpt: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyScheduledAgentOccurrenceInput {
    pub task_id: ScheduledTaskId,
    pub scheduled_for: UnixTimestampMilliseconds,
    pub created_at: UnixTimestampMilliseconds,
    pub prompt: String,
    pub model_reference: String,
    pub updated_at: UnixTimestampMilliseconds,
    pub disposition: LegacyScheduledAgentOccurrenceDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyScheduledAgentCutoverProjection {
    pub stage: LegacyScheduledAgentCutoverStage,
    pub source_generation: Option<u64>,
    pub source_fingerprint: Option<ContentDigest>,
    pub backup_digest: Option<ContentDigest>,
    pub backup_byte_count: Option<u64>,
    pub task_count: u32,
    pub occurrence_count: u32,
    pub failure: Option<LegacyScheduledAgentCutoverFailure>,
}

impl LegacyScheduledAgentCutoverProjection {
    pub(super) fn from_report(report: LegacyScheduledAgentCutoverReport) -> Self {
        let (stage, source_generation) = match report.state {
            ScheduledAgentCutoverState::NotStarted => {
                (LegacyScheduledAgentCutoverStage::NotStarted, None)
            }
            ScheduledAgentCutoverState::Staged { source_generation } => (
                LegacyScheduledAgentCutoverStage::Staged,
                Some(source_generation),
            ),
            ScheduledAgentCutoverState::Verified { source_generation } => (
                LegacyScheduledAgentCutoverStage::Verified,
                Some(source_generation),
            ),
            ScheduledAgentCutoverState::Authoritative { source_generation } => (
                LegacyScheduledAgentCutoverStage::Authoritative,
                Some(source_generation),
            ),
        };
        Self {
            stage,
            source_generation,
            source_fingerprint: report.source_fingerprint,
            backup_digest: report.backup_digest,
            backup_byte_count: report.backup_byte_count,
            task_count: report.task_count,
            occurrence_count: report.occurrence_count,
            failure: None,
        }
    }

    pub(super) fn inspected(
        source_generation: u64,
        source_fingerprint: ContentDigest,
        backup_digest: ContentDigest,
        backup_byte_count: u64,
        task_count: usize,
        occurrence_count: usize,
    ) -> Self {
        Self {
            stage: LegacyScheduledAgentCutoverStage::NotStarted,
            source_generation: Some(source_generation),
            source_fingerprint: Some(source_fingerprint),
            backup_digest: Some(backup_digest),
            backup_byte_count: Some(backup_byte_count),
            task_count: task_count as u32,
            occurrence_count: occurrence_count as u32,
            failure: None,
        }
    }

    pub(super) fn blocked(error: StorageError) -> Self {
        let code = match error {
            StorageError::InvalidLegacyRecord { .. } | StorageError::ImportLimitExceeded { .. } => {
                LegacyScheduledAgentCutoverFailureCode::InvalidSource
            }
            StorageError::ScheduledAgentWorkflowConflict
            | StorageError::CutoverAlreadyAuthoritative => {
                LegacyScheduledAgentCutoverFailureCode::ConflictingCoreState
            }
            _ => LegacyScheduledAgentCutoverFailureCode::StorageUnavailable,
        };
        Self {
            stage: LegacyScheduledAgentCutoverStage::Blocked,
            source_generation: None,
            source_fingerprint: None,
            backup_digest: None,
            backup_byte_count: None,
            task_count: 0,
            occurrence_count: 0,
            failure: Some(LegacyScheduledAgentCutoverFailure {
                code,
                diagnostic_code: error.code().to_owned(),
            }),
        }
    }
}
