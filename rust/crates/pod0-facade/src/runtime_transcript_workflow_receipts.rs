use pod0_application::{HostObservationReceipt, HostObservationRejection};
use pod0_storage::StorageError;

use crate::runtime_chapter_model_receipts::{rejected, retain};

pub(super) fn storage_receipt(
    request_id: pod0_domain::HostRequestId,
    error: StorageError,
) -> HostObservationReceipt {
    match error {
        StorageError::TranscriptWorkflowConflict
        | StorageError::TranscriptWorkflowNotFound
        | StorageError::StaleTranscriptAttempt => {
            rejected(request_id, HostObservationRejection::StaleWorkflow)
        }
        _ => retain(request_id),
    }
}
