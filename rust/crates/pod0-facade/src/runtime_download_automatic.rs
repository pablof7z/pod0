use pod0_application::{
    ActivityDomain, ActivitySubject, CommandEnvelope, CoreFailureCode,
    DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS, DownloadAdmissionDecision, DownloadIntentOrigin,
    DurableInternalCommandRequest, InternalCommandKind, RequestDisposition, RequestRejectionReason,
    download_input_version, download_intent_id, evaluate_download_admission,
};
use pod0_domain::{
    AutoDownloadMode, CancellationId, CommandId, EpisodeId, InternalCommandId, PodcastId,
};
use pod0_storage::{DownloadEnsureInput, DownloadEnsureOutcome};
use sha2::{Digest as _, Sha256};

use crate::runtime_download_mapping::{environment_projection, stored_origin, wait_failure};
use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn report_automatic_download_candidates(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        podcast_id: PodcastId,
        episode_ids: Vec<EpisodeId>,
    ) {
        const MAXIMUM_CANDIDATES: usize = 200;
        let Some(store) = self.store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        if store.require_download_workflow_authoritative().is_err() {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        }
        let mut unique = std::collections::BTreeSet::new();
        if episode_ids.len() > MAXIMUM_CANDIDATES
            || episode_ids
                .iter()
                .any(|episode_id| !unique.insert(*episode_id))
        {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        }
        if let Err(error) = self.reload_listening() {
            self.fail(envelope.command_id, storage_failure(error));
            return;
        }
        let mut candidates = Vec::with_capacity(episode_ids.len());
        for episode_id in episode_ids {
            let Some(episode) = self
                .listening
                .episodes
                .iter()
                .find(|episode| {
                    episode.episode_id == episode_id && episode.podcast_id == podcast_id
                })
                .cloned()
            else {
                self.fail(envelope.command_id, CoreFailureCode::NotFound);
                return;
            };
            candidates.push(episode);
        }
        candidates.sort_by(|left, right| {
            right
                .published_at
                .value
                .cmp(&left.published_at.value)
                .then_with(|| left.episode_id.cmp(&right.episode_id))
        });
        let policy = self.download_policy(podcast_id);
        let selected = match policy.mode {
            AutoDownloadMode::Off | AutoDownloadMode::Unsupported { .. } => 0,
            AutoDownloadMode::Latest { count } => usize::from(count).min(candidates.len()),
            AutoDownloadMode::AllNew => candidates.len(),
        };
        let selected = candidates.into_iter().take(selected).collect::<Vec<_>>();
        let internal_commands = selected
            .iter()
            .map(|episode| DurableInternalCommandRequest {
                kind: InternalCommandKind::RequestEpisodeDownload {
                    origin: DownloadIntentOrigin::Automatic,
                },
                target: ActivityDomain::Download,
                subject: ActivitySubject::Episode {
                    episode_id: episode.episode_id,
                },
                episode_id: Some(episode.episode_id),
            })
            .collect();
        if let Err(error) = store.record_download_noop_command(
            envelope.command_id,
            fingerprint,
            ActivitySubject::Podcast { podcast_id },
            None,
            DownloadIntentOrigin::Automatic,
            internal_commands,
            self.now().value,
        ) {
            self.fail(envelope.command_id, storage_failure(error));
            return;
        }
        self.resume_automatic_download_commands();
        self.succeed(envelope.command_id, None);
    }

    pub(super) fn resume_automatic_download_commands(&mut self) {
        let Some(store) = self.store.clone() else {
            return;
        };
        if store.require_download_workflow_authoritative().is_err() {
            return;
        }
        let Ok(commands) = store.pending_internal_commands(100) else {
            return;
        };
        for command in commands {
            if matches!(
                command.request.kind,
                InternalCommandKind::RequestEpisodeDownload { .. }
            ) {
                self.execute_automatic_download_command(&store, command);
            }
        }
    }

    fn execute_automatic_download_command(
        &mut self,
        store: &pod0_storage::LibraryStore,
        command: pod0_storage::PendingInternalCommand,
    ) {
        let InternalCommandKind::RequestEpisodeDownload { origin } = command.request.kind else {
            return;
        };
        let Some(episode_id) = command.request.episode_id else {
            return;
        };
        let Some(episode) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
            .cloned()
        else {
            self.consume_automatic_download_command(
                store,
                command,
                episode_id,
                RequestDisposition::Rejected {
                    reason: RequestRejectionReason::MissingSubject,
                },
            );
            return;
        };
        let Some(input_version) = download_input_version(
            &episode.enclosure_url,
            episode.enclosure_mime_type.as_deref(),
            episode.duration_milliseconds,
        ) else {
            self.consume_automatic_download_command(
                store,
                command,
                episode_id,
                RequestDisposition::Rejected {
                    reason: RequestRejectionReason::Invalid,
                },
            );
            return;
        };
        let Some(intent_id) = download_intent_id(episode_id, &input_version) else {
            return;
        };
        let Ok(environment) = store.download_environment().map(environment_projection) else {
            return;
        };
        let admission = evaluate_download_admission(
            origin,
            self.download_policy(episode.podcast_id),
            environment,
        );
        if admission == DownloadAdmissionDecision::Obsolete {
            self.consume_automatic_download_command(
                store,
                command,
                episode_id,
                RequestDisposition::NoSemanticChange,
            );
            return;
        }
        let Ok(stored_origin) = stored_origin(origin) else {
            return;
        };
        let now = self.now().value;
        let Some(deadline_at_ms) = now.checked_add(DOWNLOAD_HOST_REQUEST_DEADLINE_MILLISECONDS)
        else {
            return;
        };
        let command_id = CommandId::from_bytes(command.internal_command_id.into_bytes());
        let fingerprint = internal_download_fingerprint(command.internal_command_id);
        let result = store.ensure_download_workflow_from_internal_command(
            command,
            DownloadEnsureInput {
                episode_id,
                intent_id,
                input_version,
                origin: stored_origin,
                admitted: admission == DownloadAdmissionDecision::Admit,
                wait_failure_code: wait_failure(admission).map(str::to_owned),
                command_id,
                command_fingerprint: fingerprint,
                cancellation_id: internal_download_cancellation(command_id),
                enclosure_url: episode.enclosure_url,
                issued_revision: self.revision,
                now_ms: now,
                deadline_at_ms,
            },
        );
        match result {
            Ok(DownloadEnsureOutcome::Changed { record, replaced }) => {
                if let Some(request_id) = replaced.and_then(|item| item.request_id) {
                    self.withdraw_download_request(request_id);
                }
                self.finish_download_command(command_id, record);
            }
            Ok(DownloadEnsureOutcome::Existing(record)) => {
                self.finish_download_command(command_id, record);
            }
            Err(_) => {}
        }
    }

    fn consume_automatic_download_command(
        &self,
        store: &pod0_storage::LibraryStore,
        command: pod0_storage::PendingInternalCommand,
        episode_id: EpisodeId,
        disposition: RequestDisposition,
    ) {
        let _ = store.record_download_internal_disposition(
            command,
            episode_id,
            disposition,
            self.now(),
        );
    }
}

fn internal_download_cancellation(command_id: CommandId) -> CancellationId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/automatic-download/internal-cancellation/v1");
    hash.update(command_id.into_bytes());
    CancellationId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}

fn internal_download_fingerprint(command_id: InternalCommandId) -> String {
    let mut hash = Sha256::new();
    hash.update(b"pod0/automatic-download/internal-command/v1");
    hash.update(command_id.into_bytes());
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
