use pod0_domain::{
    CancellationId, CommandId, HostRequestId, StateRevision, UnixTimestampMilliseconds,
};

/// The exact, persisted native cancellation action authorized by Rust.
///
/// `request_id` identifies this cancellation delivery. `target_request_id`
/// identifies the native work that must be cancelled. Keeping both identities
/// in the durable payload makes cancellation restart-safe and prevents the
/// native shell from inferring which work Rust intended to withdraw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableHostCancellationEffectRequest {
    pub request_id: HostRequestId,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub target_request_id: HostRequestId,
}

impl DurableHostCancellationEffectRequest {
    #[must_use]
    pub const fn to_host(self) -> crate::HostRequestEnvelope {
        crate::HostRequestEnvelope {
            request_id: self.request_id,
            command_id: self.command_id,
            cancellation_id: self.cancellation_id,
            issued_revision: self.issued_revision,
            deadline_at: None,
            request: crate::HostRequest::CancelAuthorizedEffect {
                target_request_id: self.target_request_id,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableHostCancellationObservation {
    pub request_id: HostRequestId,
    pub cancellation_id: CancellationId,
    pub observed_request_revision: StateRevision,
    pub sequence_number: u64,
    pub observed_at: UnixTimestampMilliseconds,
    pub outcome: DurableHostCancellationOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DurableHostCancellationOutcome {
    Applied { target_request_id: HostRequestId },
    Failed { code: crate::HostFailureCode },
}

impl DurableHostCancellationObservation {
    #[must_use]
    pub fn from_host(value: &crate::HostObservationEnvelope) -> Option<Self> {
        let outcome = match value.observation {
            crate::HostObservation::AuthorizedEffectCancellationApplied { target_request_id } => {
                DurableHostCancellationOutcome::Applied { target_request_id }
            }
            crate::HostObservation::Failed { code, .. } => {
                DurableHostCancellationOutcome::Failed { code }
            }
            _ => return None,
        };
        Some(Self {
            request_id: value.request_id,
            cancellation_id: value.cancellation_id,
            observed_request_revision: value.observed_request_revision,
            sequence_number: value.sequence_number,
            observed_at: value.observed_at,
            outcome,
        })
    }
}
