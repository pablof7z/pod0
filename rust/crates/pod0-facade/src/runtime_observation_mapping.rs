use pod0_application::{
    CoreFailureCode, HostFailureCode, HostObservationReceipt, HostObservationRejection,
    ObservationAcceptance,
};
use pod0_domain::HostRequestId;

pub(super) fn host_failure(code: HostFailureCode) -> CoreFailureCode {
    match code {
        HostFailureCode::PermissionDenied => CoreFailureCode::HostRejected,
        HostFailureCode::InvalidResponse | HostFailureCode::ResponseTooLarge => {
            CoreFailureCode::FeedMalformed
        }
        _ => CoreFailureCode::HostUnavailable,
    }
}

pub(super) fn accepted(request_id: HostRequestId) -> HostObservationReceipt {
    HostObservationReceipt::AcceptedTransient { request_id }
}

pub(super) fn retain(request_id: HostRequestId) -> HostObservationReceipt {
    HostObservationReceipt::RetainAndRetry { request_id }
}

pub(super) fn rejected(
    request_id: HostRequestId,
    acceptance: ObservationAcceptance,
) -> HostObservationReceipt {
    let reason = match acceptance {
        ObservationAcceptance::UnknownRequest => HostObservationRejection::UnknownRequest,
        ObservationAcceptance::Duplicate => HostObservationRejection::Duplicate,
        ObservationAcceptance::Cancelled => HostObservationRejection::Cancelled,
        ObservationAcceptance::CancellationMismatch => {
            HostObservationRejection::CancellationMismatch
        }
        ObservationAcceptance::StaleRequestRevision => {
            HostObservationRejection::StaleRequestRevision
        }
        ObservationAcceptance::OutOfOrder => HostObservationRejection::OutOfOrder,
        ObservationAcceptance::MismatchedPayload => HostObservationRejection::MismatchedPayload,
        ObservationAcceptance::PayloadTooLarge => HostObservationRejection::PayloadTooLarge,
        ObservationAcceptance::Accepted => unreachable!("accepted observations are handled above"),
    };
    HostObservationReceipt::Rejected { request_id, reason }
}
