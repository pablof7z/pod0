use crate::runtime_download_mapping::{
    environment_projection, stored_network, stored_origin, wait_failure,
};
use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;
use pod0_application::{
    CommandEnvelope, CoreFailureCode, DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS,
    DownloadAdmissionDecision, DownloadEnvironmentObservation, DownloadIntentOrigin,
    OperationStage, download_input_version, download_intent_id, evaluate_download_admission,
};
use pod0_domain::{EpisodeId, StateRevision};
use pod0_storage::{
    DownloadEnsureInput, DownloadEnsureOutcome, DownloadWorkflowRecord, StoredDownloadStage,
};

impl FacadeState {
    pub(super) fn request_episode_download(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        episode_id: EpisodeId,
        origin: DownloadIntentOrigin,
    ) {
        let Some(store) = self.store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        if store.require_download_workflow_authoritative().is_err() {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        }
        if let Err(error) = self.reload_listening() {
            self.fail(envelope.command_id, storage_failure(error));
            return;
        }
        let Some(episode) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
            .cloned()
        else {
            self.fail(envelope.command_id, CoreFailureCode::NotFound);
            return;
        };
        let Some(input_version) = download_input_version(
            &episode.enclosure_url,
            episode.enclosure_mime_type.as_deref(),
            episode.duration_milliseconds,
        ) else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        };
        let Some(intent_id) = download_intent_id(episode_id, &input_version) else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        };
        let environment = match store.download_environment() {
            Ok(value) => environment_projection(value),
            Err(error) => {
                self.fail(envelope.command_id, storage_failure(error));
                return;
            }
        };
        let policy = self.download_policy(episode.podcast_id);
        let admission = evaluate_download_admission(origin, policy, environment);
        if admission == DownloadAdmissionDecision::Obsolete {
            match store.record_download_noop_command(
                envelope.command_id,
                fingerprint,
                self.now().value,
            ) {
                Ok(_) => self.succeed(envelope.command_id, None),
                Err(error) => self.fail(envelope.command_id, storage_failure(error)),
            }
            return;
        }
        let now = self.now().value;
        let Some(deadline) = now.checked_add(DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS) else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        };
        let stored_origin = match stored_origin(origin) {
            Ok(value) => value,
            Err(code) => {
                self.fail(envelope.command_id, code);
                return;
            }
        };
        let result = store.ensure_download_workflow(DownloadEnsureInput {
            episode_id,
            intent_id,
            input_version,
            origin: stored_origin,
            admitted: admission == DownloadAdmissionDecision::Admit,
            wait_failure_code: wait_failure(admission).map(str::to_owned),
            command_id: envelope.command_id,
            command_fingerprint: fingerprint.to_owned(),
            cancellation_id: envelope.cancellation_id,
            enclosure_url: episode.enclosure_url,
            issued_revision: self.revision,
            now_ms: now,
            deadline_at_ms: deadline,
        });
        match result {
            Ok(DownloadEnsureOutcome::Changed { record, replaced }) => {
                if let Some(request_id) = replaced.and_then(|item| item.request_id) {
                    self.withdraw_download_request(request_id);
                }
                self.finish_download_command(envelope.command_id, record);
            }
            Ok(DownloadEnsureOutcome::Existing(record)) => {
                self.finish_download_command(envelope.command_id, record);
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    pub(super) fn cancel_episode_download(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        episode_id: EpisodeId,
        expected_revision: StateRevision,
    ) {
        let Some(store) = self.store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        if store.require_download_workflow_authoritative().is_err() {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        }
        let existing = store.download_workflow(episode_id).ok().flatten();
        let result = store.cancel_download_workflow(
            envelope.command_id,
            fingerprint,
            episode_id,
            expected_revision,
            self.revision,
            self.now().value,
        );
        match result {
            Ok(transition) => {
                if let Some(request_id) = existing.and_then(|item| item.request_id) {
                    self.withdraw_download_request(request_id);
                }
                let _ = self.admit_download_requests();
                if self.pending_downloads.values().any(|request| {
                    request.command_id == envelope.command_id
                        && request.kind == pod0_storage::DownloadHostRequestKind::Cancel
                }) {
                    self.finish(envelope.command_id, OperationStage::Running, None, None);
                } else {
                    self.succeed(envelope.command_id, None);
                }
                self.revision = StateRevision::new(
                    self.revision
                        .value
                        .max(transition.record.workflow_revision.value),
                );
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    pub(super) fn remove_episode_download(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        episode_id: EpisodeId,
        expected_revision: StateRevision,
    ) {
        let Some(store) = self.store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        if store.require_download_workflow_authoritative().is_err() {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        }
        let now = self.now().value;
        let Some(deadline) = now.checked_add(DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS) else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        };
        match store.remove_download_artifact(pod0_storage::DownloadRemovalInput {
            command_id: envelope.command_id,
            command_fingerprint: fingerprint.to_owned(),
            episode_id,
            expected_revision,
            issued_revision: self.revision,
            now_ms: now,
            deadline_at_ms: deadline,
        }) {
            Ok(transition) => {
                self.revision = StateRevision::new(
                    self.revision
                        .value
                        .max(transition.record.workflow_revision.value),
                );
                if self.admit_download_requests().is_ok() {
                    self.finish(envelope.command_id, OperationStage::Running, None, None);
                } else {
                    self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
                }
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    pub(super) fn observe_download_environment(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        observation: DownloadEnvironmentObservation,
    ) {
        let Some(store) = self.store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        if store.require_download_workflow_authoritative().is_err() {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        }
        let network = stored_network(observation.network);
        let result = store.observe_download_environment(
            envelope.command_id,
            fingerprint,
            network,
            observation.available_capacity_bytes,
            self.now().value,
        );
        if let Err(error) = result {
            self.fail(envelope.command_id, storage_failure(error));
            return;
        }
        if let Err(error) = self.reconcile_waiting_downloads(observation) {
            self.fail(envelope.command_id, storage_failure(error));
            return;
        }
        self.succeed(envelope.command_id, None);
    }

    pub(super) fn finish_download_command(
        &mut self,
        command_id: pod0_domain::CommandId,
        record: DownloadWorkflowRecord,
    ) {
        self.revision = StateRevision::new(self.revision.value.max(record.workflow_revision.value));
        match record.stage {
            StoredDownloadStage::Requested | StoredDownloadStage::RetryScheduled => {
                let Some(request_id) = record.request_id else {
                    self.fail(command_id, CoreFailureCode::StorageUnavailable);
                    return;
                };
                let Some(store) = self.store.clone() else {
                    self.fail(command_id, CoreFailureCode::StorageUnavailable);
                    return;
                };
                match store.download_host_request(request_id) {
                    // Dispatch commits durable work; bounded host admission happens when the
                    // native host polls. Queue pressure must never turn committed work into a
                    // false storage failure.
                    Ok(Some((_, state))) if state == "pending" => {
                        self.finish(command_id, OperationStage::Running, None, None)
                    }
                    _ => self.fail(command_id, CoreFailureCode::StorageUnavailable),
                }
            }
            StoredDownloadStage::Waiting
            | StoredDownloadStage::HostAccepted
            | StoredDownloadStage::Transferring
            | StoredDownloadStage::Staged
            | StoredDownloadStage::Removing => {
                self.finish(command_id, OperationStage::Running, None, None)
            }
            StoredDownloadStage::Succeeded | StoredDownloadStage::Cancelled => {
                self.succeed(command_id, None)
            }
            StoredDownloadStage::Failed => self.fail(command_id, CoreFailureCode::HostRejected),
        }
    }
}
