use pod0_application::{
    HostFailureCode, HostObservation, HostObservationReceipt, HostObservationRejection,
    LeasedHostObservationEnvelope,
};
use pod0_storage::{
    DownloadFailureInput, DownloadLeasedObservationAction, DownloadObservationCommitInput,
    StoredDownloadStage,
};

use crate::runtime_chapter_model_receipts::{persisted, rejected, retain, storage_receipt};
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn record_leased_download_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = leased.observation.request_id;
        let acceptance = self.host_requests.validate_observation(&leased.observation);
        if !matches!(
            acceptance,
            pod0_application::ObservationAcceptance::Accepted
                | pod0_application::ObservationAcceptance::UnknownRequest
                | pod0_application::ObservationAcceptance::Duplicate
        ) {
            return (
                false,
                crate::runtime_observation_mapping::rejected(request_id, acceptance),
            );
        }
        let Some(store) = self.store.clone() else {
            return (false, retain(request_id));
        };
        let request = match store.effect_request(leased.lease.intent_id) {
            Ok(Some(pod0_application::DurableExternalEffectRequest {
                execution: pod0_application::DurableEffectExecution::Download { request },
                ..
            })) if request.request_id == request_id => request,
            Ok(_) => {
                return (
                    false,
                    rejected(request_id, HostObservationRejection::StaleWorkflow),
                );
            }
            Err(_) => return (false, retain(request_id)),
        };
        let record = match store.download_host_request(request_id) {
            Ok(Some((record, state)))
                if state == "pending" && record.episode_id == request.episode_id() =>
            {
                record
            }
            Ok(_) => {
                return (
                    false,
                    rejected(request_id, HostObservationRejection::StaleWorkflow),
                );
            }
            Err(_) => return (false, retain(request_id)),
        };
        let action = match download_action(&leased.observation, self.revision) {
            Some(action) => action,
            None => {
                return (
                    false,
                    rejected(request_id, HostObservationRejection::MismatchedPayload),
                );
            }
        };
        let staged = matches!(
            &leased.observation.observation,
            HostObservation::DownloadStaged { .. }
        );
        let committed = match store.commit_download_observation(DownloadObservationCommitInput {
            lease: leased.lease,
            observation: leased.observation,
            action,
            committed_at: self.now(),
        }) {
            Ok(value) => value,
            Err(error) => return (false, storage_receipt(request_id, error)),
        };
        if committed.replayed {
            return (
                false,
                rejected(request_id, HostObservationRejection::Duplicate),
            );
        }
        if committed.terminal_effect {
            self.retire_download_request(request_id);
        }
        self.revision = pod0_domain::StateRevision::new(
            self.revision
                .value
                .max(committed.workflow.workflow_revision.value),
        );
        self.finish_download_operation(&record, &committed.workflow);
        if staged {
            return self.finish_staged_download(request_id, &record);
        }
        if matches!(
            committed.workflow.stage,
            StoredDownloadStage::Succeeded
                | StoredDownloadStage::Cancelled
                | StoredDownloadStage::Failed
        ) {
            let _ = self.reload_listening();
        }
        (true, persisted(request_id, committed.terminal_effect))
    }

    fn finish_staged_download(
        &mut self,
        request_id: pod0_domain::HostRequestId,
        record: &pod0_storage::DownloadHostRequestRecord,
    ) -> (bool, HostObservationReceipt) {
        let Some(store) = self.store.clone() else {
            return (false, retain(request_id));
        };
        match store.finalize_pending_download_artifact(request_id, self.now()) {
            Ok(Some(workflow)) => {
                self.retire_download_request(request_id);
                self.revision = pod0_domain::StateRevision::new(
                    self.revision.value.max(workflow.workflow_revision.value),
                );
                self.finish_download_operation(record, &workflow);
                let _ = self.reload_listening();
                (true, persisted(request_id, true))
            }
            Ok(None) => (
                false,
                rejected(request_id, HostObservationRejection::StaleWorkflow),
            ),
            Err(_) => (false, retain(request_id)),
        }
    }
}

fn download_action(
    envelope: &pod0_application::HostObservationEnvelope,
    issued_revision: pod0_domain::StateRevision,
) -> Option<DownloadLeasedObservationAction> {
    match &envelope.observation {
        HostObservation::DownloadAccepted {
            external_task_key,
            resume_key,
            ..
        } => Some(DownloadLeasedObservationAction::Accepted {
            external_task_key: external_task_key.clone(),
            resume_key: resume_key.clone(),
        }),
        HostObservation::DownloadCancelled { .. } | HostObservation::Cancelled => {
            Some(DownloadLeasedObservationAction::Cancellation)
        }
        HostObservation::DownloadArtifactRemoved { artifact_key, .. } => {
            Some(DownloadLeasedObservationAction::Removal {
                artifact_key: artifact_key.clone(),
            })
        }
        HostObservation::DownloadStaged {
            staged_file_path,
            byte_count,
            ..
        } => Some(DownloadLeasedObservationAction::Staged {
            staged_file_path: staged_file_path.clone(),
            claimed_byte_count: *byte_count,
        }),
        HostObservation::Failed { code, safe_detail } => {
            let (failure_code, retryable) = classify_failure(*code);
            let retry_at = retryable
                .then(|| pod0_application::download_retry_not_before(envelope.observed_at).value);
            Some(DownloadLeasedObservationAction::Failure(
                DownloadFailureInput {
                    request_id: envelope.request_id,
                    sequence_number: envelope.sequence_number,
                    failure_code: failure_code.to_owned(),
                    failure_detail: safe_detail.clone(),
                    retryable,
                    retry_at_ms: retry_at,
                    retry_deadline_at_ms: retry_at.and_then(|value| {
                        value.checked_add(
                            pod0_application::DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS,
                        )
                    }),
                    issued_revision,
                    observed_at_ms: envelope.observed_at.value,
                },
            ))
        }
        HostObservation::Unsupported { wire_code } => Some(
            DownloadLeasedObservationAction::Failure(DownloadFailureInput {
                request_id: envelope.request_id,
                sequence_number: envelope.sequence_number,
                failure_code: "host_rejected".to_owned(),
                failure_detail: Some(format!("unsupported host observation {wire_code}")),
                retryable: false,
                retry_at_ms: None,
                retry_deadline_at_ms: None,
                issued_revision,
                observed_at_ms: envelope.observed_at.value,
            }),
        ),
        _ => None,
    }
}

fn classify_failure(code: HostFailureCode) -> (&'static str, bool) {
    match code {
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
    }
}
