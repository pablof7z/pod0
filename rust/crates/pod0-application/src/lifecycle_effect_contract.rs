use pod0_domain::{CancellationId, CommandId, HostRequestId, StateRevision};

use crate::{CoreWakeReason, HostRequest, HostRequestEnvelope};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableLifecycleEffectRequest {
    pub request_id: HostRequestId,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub wake_at: pod0_domain::UnixTimestampMilliseconds,
    pub reason: CoreWakeReason,
    pub attempt: u8,
}

impl DurableLifecycleEffectRequest {
    #[must_use]
    pub fn to_host(&self) -> HostRequestEnvelope {
        HostRequestEnvelope {
            request_id: self.request_id,
            command_id: self.command_id,
            cancellation_id: self.cancellation_id,
            issued_revision: self.issued_revision,
            deadline_at: None,
            request: HostRequest::ScheduleCoreWake {
                wake_at: self.wake_at,
                reason: self.reason,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableLifecycleHostObservation {
    pub request_id: HostRequestId,
    pub cancellation_id: CancellationId,
    pub observed_request_revision: StateRevision,
    pub sequence_number: u64,
    pub observed_at: pod0_domain::UnixTimestampMilliseconds,
    pub outcome: LifecycleWakeOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LifecycleWakeOutcome {
    Reached { reason: CoreWakeReason },
    Failed { code: crate::HostFailureCode },
    Cancelled,
}

impl DurableLifecycleHostObservation {
    #[must_use]
    pub fn from_host(value: &crate::HostObservationEnvelope) -> Option<Self> {
        let outcome = match value.observation {
            crate::HostObservation::CoreWakeReached { reason } => {
                LifecycleWakeOutcome::Reached { reason }
            }
            crate::HostObservation::Failed { code, .. } => LifecycleWakeOutcome::Failed { code },
            crate::HostObservation::Cancelled => LifecycleWakeOutcome::Cancelled,
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
