use pod0_domain::{CommandId, ContentDigest, EpisodeId, PodcastId, UnixTimestampMilliseconds};
use pod0_storage::{FeedDiscoveryCutoverState, LegacyFeedDiscoveryCutoverReport, StorageError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyFeedDiscoveryCutoverStage {
    NotStarted,
    Staged,
    Authoritative,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyFeedDiscoveryCutoverFailureCode {
    InvalidSource,
    ConflictingCoreState,
    StorageUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyFeedDiscoveryCutoverFailure {
    pub code: LegacyFeedDiscoveryCutoverFailureCode,
    pub diagnostic_code: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyFeedDiscoveryEffectKindInput {
    Download,
    Notification,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LegacyFeedDiscoveryDispositionInput {
    Pending {
        attempt: u8,
        not_before: Option<UnixTimestampMilliseconds>,
    },
    Succeeded {
        attempt: u8,
    },
    Obsolete {
        attempt: u8,
    },
    Failed {
        attempt: u8,
    },
    Ambiguous {
        attempt: u8,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyFeedDiscoveryCandidateInput {
    pub source_occurrence_id: CommandId,
    pub podcast_id: PodcastId,
    pub episode_id: EpisodeId,
    pub kind: LegacyFeedDiscoveryEffectKindInput,
    pub disposition: LegacyFeedDiscoveryDispositionInput,
    pub observed_at: UnixTimestampMilliseconds,
    pub expires_at: UnixTimestampMilliseconds,
    pub published_at: UnixTimestampMilliseconds,
    pub input_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LegacyFeedDiscoveryCutoverProjection {
    pub stage: LegacyFeedDiscoveryCutoverStage,
    pub source_generation: Option<u64>,
    pub source_fingerprint: Option<ContentDigest>,
    pub backup_digest: Option<ContentDigest>,
    pub backup_byte_count: Option<u64>,
    pub notifications_enabled: Option<bool>,
    pub inspected_job_count: u32,
    pub candidate_count: u32,
    pub blocked_count: u32,
    pub ambiguous_count: u32,
    pub failure: Option<LegacyFeedDiscoveryCutoverFailure>,
}

impl LegacyFeedDiscoveryCutoverProjection {
    pub(super) fn from_report(report: LegacyFeedDiscoveryCutoverReport) -> Self {
        let (stage, source_generation) = match report.state {
            FeedDiscoveryCutoverState::NotStarted => {
                (LegacyFeedDiscoveryCutoverStage::NotStarted, None)
            }
            FeedDiscoveryCutoverState::Staged { source_generation } => (
                LegacyFeedDiscoveryCutoverStage::Staged,
                Some(source_generation),
            ),
            FeedDiscoveryCutoverState::Authoritative { source_generation } => (
                LegacyFeedDiscoveryCutoverStage::Authoritative,
                Some(source_generation),
            ),
        };
        Self {
            stage,
            source_generation,
            source_fingerprint: report.source_fingerprint,
            backup_digest: report.backup_digest,
            backup_byte_count: report.backup_byte_count,
            notifications_enabled: report.notifications_enabled,
            inspected_job_count: report.inspected_job_count,
            candidate_count: report.candidate_count,
            blocked_count: report.blocked_count,
            ambiguous_count: report.ambiguous_count,
            failure: None,
        }
    }

    pub(super) fn inspected(
        input: &pod0_storage::LegacyFeedDiscoveryCutoverInput,
        source_fingerprint: ContentDigest,
        source_generation: u64,
    ) -> Self {
        Self {
            stage: LegacyFeedDiscoveryCutoverStage::NotStarted,
            source_generation: Some(source_generation),
            source_fingerprint: Some(source_fingerprint),
            backup_digest: Some(input.backup_digest),
            backup_byte_count: Some(input.backup_byte_count),
            notifications_enabled: Some(input.notifications_enabled),
            inspected_job_count: input.inspected_job_count,
            candidate_count: input.candidates.len() as u32,
            blocked_count: input.blocked_count,
            ambiguous_count: input.ambiguous_count,
            failure: None,
        }
    }

    pub(super) fn blocked(error: StorageError) -> Self {
        let code = match error {
            StorageError::InvalidFeedDiscoveryCutover => {
                LegacyFeedDiscoveryCutoverFailureCode::InvalidSource
            }
            StorageError::FeedDiscoveryCutoverConflict
            | StorageError::CutoverAlreadyAuthoritative => {
                LegacyFeedDiscoveryCutoverFailureCode::ConflictingCoreState
            }
            _ => LegacyFeedDiscoveryCutoverFailureCode::StorageUnavailable,
        };
        Self {
            stage: LegacyFeedDiscoveryCutoverStage::Blocked,
            source_generation: None,
            source_fingerprint: None,
            backup_digest: None,
            backup_byte_count: None,
            notifications_enabled: None,
            inspected_job_count: 0,
            candidate_count: 0,
            blocked_count: 0,
            ambiguous_count: 0,
            failure: Some(LegacyFeedDiscoveryCutoverFailure {
                code,
                diagnostic_code: error.code().to_owned(),
            }),
        }
    }
}
