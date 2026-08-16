use pod0_domain::HostRequestId;

use super::{
    apply_download_artifact_removal, apply_download_cancellation, apply_download_failure,
    apply_download_host_task,
};
use crate::{DownloadFailureInput, DownloadObservationOutcome, LibraryStore, StorageError};

impl LibraryStore {
    pub fn accept_download_host_task(
        &self,
        request_id: HostRequestId,
        sequence_number: u64,
        external_task_key: &str,
        resume_key: Option<&str>,
        observed_at_ms: i64,
    ) -> Result<DownloadObservationOutcome, StorageError> {
        self.write(|transaction| {
            apply_download_host_task(
                transaction,
                request_id,
                sequence_number,
                external_task_key,
                resume_key,
                observed_at_ms,
            )
        })
    }

    pub fn complete_download_cancellation(
        &self,
        request_id: HostRequestId,
        sequence_number: u64,
        observed_at_ms: i64,
    ) -> Result<DownloadObservationOutcome, StorageError> {
        self.write(|transaction| {
            apply_download_cancellation(transaction, request_id, sequence_number, observed_at_ms)
        })
    }

    pub fn complete_download_artifact_removal(
        &self,
        request_id: HostRequestId,
        sequence_number: u64,
        artifact_key: &str,
        observed_at_ms: i64,
    ) -> Result<DownloadObservationOutcome, StorageError> {
        self.write(|transaction| {
            apply_download_artifact_removal(
                transaction,
                request_id,
                sequence_number,
                artifact_key,
                observed_at_ms,
            )
        })
    }

    pub fn fail_download_host_request(
        &self,
        input: DownloadFailureInput,
    ) -> Result<DownloadObservationOutcome, StorageError> {
        self.write(|transaction| apply_download_failure(transaction, input))
    }
}
