use pod0_application::{
    CoreFailureCode, DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS, HostFailureCode, HostObservation,
    HostObservationEnvelope, HostObservationReceipt, HostObservationRejection, OperationStage,
    download_retry_not_before,
};
use pod0_storage::{
    DownloadFailureInput, DownloadHostRequestKind, DownloadObservationOutcome, StoredDownloadStage,
};

use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn persist_download_observation(
        &mut self,
        envelope: HostObservationEnvelope,
    ) -> HostObservationReceipt {
        let request_id = envelope.request_id;
        let Some(pending) = self.pending_downloads.get(&request_id).cloned() else {
            return rejected(request_id, HostObservationRejection::UnknownRequest);
        };
        let Some(store) = self.store.clone() else {
            return retain(request_id);
        };
        let terminal = !matches!(
            envelope.observation,
            HostObservation::DownloadAccepted { .. }
        );
        let outcome = match &envelope.observation {
            HostObservation::DownloadAccepted {
                external_task_key,
                resume_key,
                ..
            } => store.accept_download_host_task(
                request_id,
                envelope.sequence_number,
                external_task_key,
                resume_key.as_deref(),
                envelope.observed_at.value,
            ),
            HostObservation::DownloadStaged {
                staged_file_path,
                byte_count,
                ..
            } => store.complete_download_from_staged_file(
                request_id,
                envelope.sequence_number,
                staged_file_path,
                *byte_count,
                envelope.observed_at.value,
            ),
            HostObservation::DownloadCancelled { .. } | HostObservation::Cancelled => store
                .complete_download_cancellation(
                    request_id,
                    envelope.sequence_number,
                    envelope.observed_at.value,
                ),
            HostObservation::DownloadArtifactRemoved { artifact_key, .. } => store
                .complete_download_artifact_removal(
                    request_id,
                    envelope.sequence_number,
                    artifact_key,
                    envelope.observed_at.value,
                ),
            HostObservation::Failed { code, safe_detail } => {
                self.fail_download_observation(&store, &envelope, *code, safe_detail.clone())
            }
            HostObservation::Unsupported { wire_code } => {
                store.fail_download_host_request(failure_input(
                    &envelope,
                    "host_rejected",
                    Some(format!("unsupported host observation {wire_code}")),
                    false,
                    self.revision,
                ))
            }
            _ => {
                return rejected(request_id, HostObservationRejection::MismatchedPayload);
            }
        };
        match outcome {
            Ok(DownloadObservationOutcome::Updated(record)) => {
                self.revision = pod0_domain::StateRevision::new(
                    self.revision.value.max(record.workflow_revision.value),
                );
                if terminal {
                    self.retire_download_request(request_id);
                }
                self.finish_download_operation(&pending, &record);
                if matches!(
                    record.stage,
                    StoredDownloadStage::Succeeded
                        | StoredDownloadStage::Cancelled
                        | StoredDownloadStage::Failed
                ) {
                    let _ = self.reload_listening();
                }
                HostObservationReceipt::Persisted {
                    request_id,
                    terminal,
                }
            }
            Ok(DownloadObservationOutcome::Duplicate(_)) => HostObservationReceipt::Persisted {
                request_id,
                terminal,
            },
            Ok(DownloadObservationOutcome::Stale) => {
                rejected(request_id, HostObservationRejection::StaleWorkflow)
            }
            Err(_) => retain(request_id),
        }
    }

    fn fail_download_observation(
        &self,
        store: &pod0_storage::LibraryStore,
        envelope: &HostObservationEnvelope,
        code: HostFailureCode,
        safe_detail: Option<String>,
    ) -> Result<DownloadObservationOutcome, pod0_storage::StorageError> {
        let (failure_code, retryable) = match code {
            HostFailureCode::Offline => ("offline", true),
            HostFailureCode::TimedOut => ("timed_out", true),
            HostFailureCode::PermissionDenied | HostFailureCode::Unauthorized => {
                ("permission_denied", false)
            }
            HostFailureCode::InvalidResponse | HostFailureCode::ResponseTooLarge => {
                ("host_rejected", false)
            }
            HostFailureCode::ProviderUnavailable
            | HostFailureCode::MediaUnavailable
            | HostFailureCode::IndexUnavailable
            | HostFailureCode::PlatformFailure => ("transport", true),
            HostFailureCode::Unsupported { .. } => ("host_rejected", false),
        };
        store.fail_download_host_request(failure_input(
            envelope,
            failure_code,
            safe_detail,
            retryable,
            self.revision,
        ))
    }

    pub(super) fn finish_download_operation(
        &mut self,
        pending: &pod0_storage::DownloadHostRequestRecord,
        record: &pod0_storage::DownloadWorkflowRecord,
    ) {
        match record.stage {
            StoredDownloadStage::Requested
            | StoredDownloadStage::HostAccepted
            | StoredDownloadStage::Transferring
            | StoredDownloadStage::Staged
            | StoredDownloadStage::RetryScheduled
            | StoredDownloadStage::Removing
            | StoredDownloadStage::Waiting => {
                self.finish(pending.command_id, OperationStage::Running, None, None)
            }
            StoredDownloadStage::Succeeded => self.succeed(pending.command_id, None),
            StoredDownloadStage::Cancelled => {
                if pending.kind == DownloadHostRequestKind::Cancel {
                    self.succeed(pending.command_id, None);
                    self.finish(
                        record.command_id,
                        OperationStage::Cancelled,
                        Some(failure(CoreFailureCode::Cancelled)),
                        None,
                    );
                } else {
                    self.finish(
                        pending.command_id,
                        OperationStage::Cancelled,
                        Some(failure(CoreFailureCode::Cancelled)),
                        None,
                    );
                }
            }
            StoredDownloadStage::Failed => {
                let code = match record.failure_code.as_deref() {
                    Some("offline" | "timed_out" | "transport") => CoreFailureCode::HostUnavailable,
                    Some("permission_denied" | "host_rejected" | "invalid_artifact") => {
                        CoreFailureCode::HostRejected
                    }
                    _ => CoreFailureCode::StorageUnavailable,
                };
                self.fail(pending.command_id, code);
            }
        }
    }
}

fn failure_input(
    envelope: &HostObservationEnvelope,
    code: &str,
    detail: Option<String>,
    retryable: bool,
    issued_revision: pod0_domain::StateRevision,
) -> DownloadFailureInput {
    let retry_at = retryable.then(|| download_retry_not_before(envelope.observed_at).value);
    let deadline =
        retry_at.and_then(|value| value.checked_add(DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS));
    DownloadFailureInput {
        request_id: envelope.request_id,
        sequence_number: envelope.sequence_number,
        failure_code: code.to_owned(),
        failure_detail: detail,
        retryable,
        retry_at_ms: retry_at,
        retry_deadline_at_ms: deadline,
        issued_revision,
        observed_at_ms: envelope.observed_at.value,
    }
}

fn retain(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    HostObservationReceipt::RetainAndRetry { request_id }
}

fn rejected(
    request_id: pod0_domain::HostRequestId,
    reason: HostObservationRejection,
) -> HostObservationReceipt {
    HostObservationReceipt::Rejected { request_id, reason }
}
