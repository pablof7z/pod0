use pod0_application::{HostObservationReceipt, HostObservationRejection, ObservationAcceptance};
use pod0_domain::HostRequestId;

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
