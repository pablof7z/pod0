use pod0_domain::{CancellationId, HostRequestId};

/// An exact, core-owned request withdrawal. The native shell cancels only the
/// matching platform task and does not infer product policy from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HostCancellationRequest {
    pub request_id: HostRequestId,
    pub cancellation_id: CancellationId,
}
