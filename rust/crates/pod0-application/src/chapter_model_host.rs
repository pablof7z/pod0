use pod0_domain::{HostRequestId, UnixTimestampMilliseconds};

use crate::ChapterModelResponseFormat;

/// The minimum provider request a native host needs to execute. Durable
/// workflow identity, provenance, and qualification inputs stay in Rust.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterModelExecutionRequest {
    pub provider: String,
    pub model: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub response_format: ChapterModelResponseFormat,
    pub maximum_completion_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterModelProviderUpdate {
    pub provider_operation_id: String,
    pub provider_status: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChapterModelCompletionObservation {
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
    /// Provider-supplied generation time when the provider actually exposes
    /// one. Rust assigns kernel time when this evidence is absent.
    pub provider_generated_at: Option<UnixTimestampMilliseconds>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChapterModelHostFailureCode {
    MissingCredential,
    InvalidRequest,
    UnsupportedProvider,
    HttpResponse { status_code: u16 },
    Offline,
    TimedOut,
    Transport,
    ResponseTooLarge,
    InvalidResponse,
    ProviderRecoveryUnavailable,
    Cancelled,
    Unsupported { wire_code: u32 },
}

/// Tells a native capability host whether an observation may be discarded.
/// Paid evidence is safe to discard only after `Persisted`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HostObservationReceipt {
    AcceptedTransient {
        request_id: HostRequestId,
    },
    Persisted {
        request_id: HostRequestId,
        terminal: bool,
    },
    RetainAndRetry {
        request_id: HostRequestId,
    },
    Rejected {
        request_id: HostRequestId,
        reason: HostObservationRejection,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HostObservationRejection {
    UnknownRequest,
    Duplicate,
    Cancelled,
    CancellationMismatch,
    StaleRequestRevision,
    OutOfOrder,
    MismatchedPayload,
    PayloadTooLarge,
    StaleWorkflow,
}
