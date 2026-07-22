use pod0_domain::{
    AutoDownloadMode, AutoDownloadPolicy, DownloadAttemptId, DownloadIntentId, EpisodeId,
    HostRequestId, StateRevision, UnixTimestampMilliseconds,
};
use sha2::{Digest as _, Sha256};

pub const DOWNLOAD_WORKFLOW_POLICY_VERSION: u32 = 1;
pub const DOWNLOAD_MINIMUM_FREE_CAPACITY_BYTES: u64 = 256 * 1_024 * 1_024;
pub const DOWNLOAD_RETRY_DELAY_MILLISECONDS: i64 = 5 * 60 * 1_000;
pub const DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS: i64 = 24 * 60 * 60 * 1_000;
pub const MAX_ACTIVE_DOWNLOAD_WORKFLOWS: u16 = 200;
pub const MAX_DOWNLOAD_ENCLOSURE_URL_BYTES: usize = 4_096;
pub const MAX_DOWNLOAD_OPAQUE_KEY_BYTES: usize = 1_024;
pub const MAX_DOWNLOAD_SAFE_DETAIL_BYTES: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DownloadIntentOrigin {
    User,
    Playback,
    Automatic,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DownloadNetworkState {
    Unknown,
    Unavailable,
    Wifi,
    Other,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DownloadEnvironmentObservation {
    pub network: DownloadNetworkState,
    pub available_capacity_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DownloadWaitReason {
    NetworkUnknown,
    NetworkUnavailable,
    WifiRequired,
    InsufficientStorage,
    UnsupportedEnvironment { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DownloadAdmissionDecision {
    Admit,
    Wait { reason: DownloadWaitReason },
    Obsolete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DownloadDesiredState {
    Present,
    Absent,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DownloadWorkflowStage {
    WaitingForEnvironment,
    Requested,
    HostAccepted,
    Transferring,
    Staged,
    RetryScheduled,
    Removing,
    Cancelled,
    Failed,
    Succeeded,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DownloadWorkflowFailureCode {
    Offline,
    WifiRequired,
    InsufficientStorage,
    MissingEpisode,
    InvalidEnclosure,
    StaleInput,
    HostRejected,
    Transport,
    TimedOut,
    PermissionDenied,
    InvalidArtifact,
    StorageUnavailable,
    Cancelled,
    RetryExhausted,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DownloadWorkflowFailure {
    pub code: DownloadWorkflowFailureCode,
    pub safe_detail: Option<String>,
    pub retryable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DownloadWorkflowAllowedActions {
    pub can_retry: bool,
    pub can_cancel: bool,
    pub can_remove: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DownloadWorkflowProjection {
    pub episode_id: EpisodeId,
    pub intent_id: DownloadIntentId,
    pub input_version: String,
    pub origin: DownloadIntentOrigin,
    pub desired_state: DownloadDesiredState,
    pub stage: DownloadWorkflowStage,
    pub workflow_revision: StateRevision,
    pub attempt: u16,
    pub attempt_id: Option<DownloadAttemptId>,
    pub request_id: Option<HostRequestId>,
    pub not_before: Option<UnixTimestampMilliseconds>,
    pub failure: Option<DownloadWorkflowFailure>,
    pub updated_at: UnixTimestampMilliseconds,
    pub allowed_actions: DownloadWorkflowAllowedActions,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DownloadWorkflowsProjection {
    pub workflows: Vec<DownloadWorkflowProjection>,
    pub has_more: bool,
    pub failure: Option<crate::CoreFailure>,
}

impl DownloadWorkflowsProjection {
    pub fn enforce_bounds(&mut self, offset: usize, requested_items: usize) {
        let limit = requested_items.clamp(
            1,
            usize::from(MAX_ACTIVE_DOWNLOAD_WORKFLOWS.min(crate::MAX_PROJECTION_ITEMS)),
        );
        let count = self.workflows.len();
        self.workflows = self.workflows.drain(..).skip(offset).take(limit).collect();
        self.has_more |= count > offset.saturating_add(self.workflows.len());
    }
}

/// Computes the kernel-owned retry boundary from an injected observation time.
/// Native hosts schedule the returned instant but never choose retry timing.
#[must_use]
pub const fn download_retry_not_before(
    observed_at: UnixTimestampMilliseconds,
) -> UnixTimestampMilliseconds {
    UnixTimestampMilliseconds::new(
        observed_at
            .value()
            .saturating_add(DOWNLOAD_RETRY_DELAY_MILLISECONDS),
    )
}

#[must_use]
pub fn evaluate_download_admission(
    origin: DownloadIntentOrigin,
    automatic_policy: AutoDownloadPolicy,
    environment: DownloadEnvironmentObservation,
) -> DownloadAdmissionDecision {
    if matches!(origin, DownloadIntentOrigin::Unsupported { .. }) {
        return DownloadAdmissionDecision::Obsolete;
    }
    if origin == DownloadIntentOrigin::Automatic {
        if matches!(automatic_policy.mode, AutoDownloadMode::Off) {
            return DownloadAdmissionDecision::Obsolete;
        }
        if automatic_policy.wifi_only && environment.network != DownloadNetworkState::Wifi {
            return DownloadAdmissionDecision::Wait {
                reason: DownloadWaitReason::WifiRequired,
            };
        }
    }
    let network_wait = match environment.network {
        DownloadNetworkState::Unknown => Some(DownloadWaitReason::NetworkUnknown),
        DownloadNetworkState::Unavailable => Some(DownloadWaitReason::NetworkUnavailable),
        DownloadNetworkState::Wifi | DownloadNetworkState::Other => None,
        DownloadNetworkState::Unsupported { wire_code } => {
            Some(DownloadWaitReason::UnsupportedEnvironment { wire_code })
        }
    };
    if let Some(reason) = network_wait {
        return DownloadAdmissionDecision::Wait { reason };
    }
    if environment
        .available_capacity_bytes
        .is_some_and(|capacity| capacity < DOWNLOAD_MINIMUM_FREE_CAPACITY_BYTES)
    {
        return DownloadAdmissionDecision::Wait {
            reason: DownloadWaitReason::InsufficientStorage,
        };
    }
    DownloadAdmissionDecision::Admit
}

#[must_use]
pub fn download_input_version(
    enclosure_url: &str,
    enclosure_mime_type: Option<&str>,
    duration_milliseconds: Option<u64>,
) -> Option<String> {
    let normalized = crate::normalize_media_url(enclosure_url)?;
    if normalized.len() > MAX_DOWNLOAD_ENCLOSURE_URL_BYTES {
        return None;
    }
    let mut hash = FramedHash::new(b"pod0-download-input-v1");
    hash.string(&normalized);
    hash.string(enclosure_mime_type.unwrap_or_default());
    hash.u64(duration_milliseconds.unwrap_or(0));
    Some(hash.hex())
}

#[must_use]
pub fn download_intent_id(episode_id: EpisodeId, input_version: &str) -> Option<DownloadIntentId> {
    if input_version.len() != 64 || !input_version.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut hash = FramedHash::new(b"pod0-download-intent-v1");
    hash.bytes(&episode_id.into_bytes());
    hash.string(input_version);
    Some(DownloadIntentId::from_bytes(hash.first_16()))
}

#[must_use]
pub fn download_attempt_id(intent_id: DownloadIntentId, attempt: u16) -> Option<DownloadAttemptId> {
    pod0_domain::download_attempt_identity(intent_id, attempt)
}

struct FramedHash(Sha256);

impl FramedHash {
    fn new(domain: &[u8]) -> Self {
        let mut value = Self(Sha256::new());
        value.bytes(domain);
        value
    }

    fn bytes(&mut self, value: &[u8]) {
        self.0.update((value.len() as u64).to_be_bytes());
        self.0.update(value);
    }

    fn string(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_be_bytes());
    }

    fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }

    fn hex(self) -> String {
        self.finish()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn first_16(self) -> [u8; 16] {
        self.finish()[..16].try_into().expect("digest slice")
    }
}
