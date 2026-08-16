use pod0_application::DurableEffectExecution;
use pod0_domain::{CancellationId, HostRequestId};

pub(super) fn cancellation_identity(
    execution: &DurableEffectExecution,
) -> Option<(CancellationId, HostRequestId)> {
    match execution {
        DurableEffectExecution::Playback { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::Download { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::Feed { request } => id(request.cancellation_id, request.request_id),
        DurableEffectExecution::AgentRecall { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::RecallQuery { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::RecallIndexCutover { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::EvidenceEmbedding { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::Lifecycle { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::AgentModel { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::AgentApproval { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::AgentCapability { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::ScheduledAgent { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::Transcript { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::PublisherChapter { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::ModelChapter { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::LibraryNetwork { request } => {
            id(request.cancellation_id, request.request_id)
        }
        DurableEffectExecution::LegacyDomainDerived
        | DurableEffectExecution::Publication { .. }
        | DurableEffectExecution::Cancellation { .. } => None,
    }
}

const fn id(
    cancellation: CancellationId,
    request: HostRequestId,
) -> Option<(CancellationId, HostRequestId)> {
    Some((cancellation, request))
}
