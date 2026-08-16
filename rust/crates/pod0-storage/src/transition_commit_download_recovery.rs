use pod0_application::{
    DownloadEffectAuthorization, DownloadRecoveryActivityInput, plan_download_recovery,
};
use pod0_domain::{CommandId, ContentDigest, EpisodeId, StateRevision, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{
    DownloadFailureInput, DownloadObservationOutcome, DownloadWorkflowTransition, StorageError,
    StoredDownloadStage, TransitionIngress, TransitionIngressKind,
};

impl crate::LibraryStore {
    pub fn reconcile_download_timeout(
        &self,
        input: DownloadFailureInput,
    ) -> Result<DownloadObservationOutcome, StorageError> {
        commit_download_timeout(self.path(), input)
    }
}

pub(crate) fn commit_waiting_download_reconciliation(
    path: &std::path::Path,
    episode_id: EpisodeId,
    expected_revision: StateRevision,
    issued_revision: StateRevision,
    now_ms: i64,
    deadline_at_ms: Option<i64>,
) -> Result<DownloadWorkflowTransition, StorageError> {
    let identity = identity(
        episode_id,
        expected_revision,
        issued_revision,
        now_ms,
        deadline_at_ms,
    );
    let prior = std::cell::RefCell::new(None);
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::Recovery,
            id: identity.into_bytes(),
            fingerprint: fingerprint(
                episode_id,
                expected_revision,
                issued_revision,
                now_ms,
                deadline_at_ms,
            ),
        },
        UnixTimestampMilliseconds::new(now_ms),
        |transaction| {
            let current = crate::download_store_read::workflow(transaction, episode_id)?
                .ok_or(StorageError::DownloadWorkflowNotFound)?;
            if current.stage != StoredDownloadStage::Waiting
                || current.workflow_revision != expected_revision
            {
                return Err(StorageError::DownloadWorkflowConflict);
            }
            *prior.borrow_mut() = Some(current.clone());
            plan_download_recovery(DownloadRecoveryActivityInput {
                identity_command_id: identity,
                command_id: current.command_id,
                episode_id,
                current_revision: current.workflow_revision,
                transition: if deadline_at_ms.is_some() {
                    pod0_application::DownloadTransition::AttemptStateChanged
                } else {
                    pod0_application::DownloadTransition::DesiredStateChanged
                },
                effect: deadline_at_ms
                    .map(|deadline| {
                        crate::download_effect_request::start_for_existing(
                            &current,
                            issued_revision,
                            None,
                            deadline,
                        )
                        .map(|request| DownloadEffectAuthorization { request })
                    })
                    .transpose()?,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, ()| {
            let transition = if let Some(deadline) = deadline_at_ms {
                crate::download_store::apply_waiting_download_admission(
                    transaction,
                    episode_id,
                    expected,
                    issued_revision,
                    now_ms,
                    deadline,
                )?
            } else {
                crate::download_store::apply_obsolete_waiting_download(
                    transaction,
                    episode_id,
                    expected,
                    now_ms,
                )?
            };
            super::download::retire_download_effects(transaction, episode_id)?;
            Ok(transition.record.workflow_revision)
        },
    )?;
    let record = crate::LibraryStore::open_authoritative(path)?
        .download_workflow(episode_id)?
        .ok_or(StorageError::DownloadWorkflowNotFound)?;
    Ok(DownloadWorkflowTransition {
        record,
        replaced: prior.borrow_mut().take().map(Box::new),
    })
}

fn commit_download_timeout(
    path: &std::path::Path,
    input: DownloadFailureInput,
) -> Result<DownloadObservationOutcome, StorageError> {
    let identity = timeout_identity(input.request_id, input.sequence_number);
    let planning_input = input.clone();
    let episode = std::cell::Cell::new(None);
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::Recovery,
            id: identity.into_bytes(),
            fingerprint: timeout_fingerprint(&input),
        },
        UnixTimestampMilliseconds::new(input.observed_at_ms),
        |transaction| {
            let (host, state) =
                crate::download_store_read::request(transaction, planning_input.request_id)?
                    .ok_or(StorageError::DownloadRequestNotFound)?;
            let current = crate::download_store_read::workflow(transaction, host.episode_id)?
                .ok_or(StorageError::DownloadWorkflowNotFound)?;
            if state != "pending" || current.request_id != Some(planning_input.request_id) {
                return Err(StorageError::DownloadWorkflowConflict);
            }
            episode.set(Some(current.episode_id));
            let retry = host.kind == crate::DownloadHostRequestKind::Start
                && planning_input.retry_at_ms.is_some()
                && planning_input.retry_deadline_at_ms.is_some();
            plan_download_recovery(DownloadRecoveryActivityInput {
                identity_command_id: identity,
                command_id: current.command_id,
                episode_id: current.episode_id,
                current_revision: current.workflow_revision,
                transition: pod0_application::DownloadTransition::AttemptStateChanged,
                effect: retry
                    .then(|| {
                        crate::download_effect_request::retry(&current, &planning_input)
                            .map(|request| DownloadEffectAuthorization { request })
                    })
                    .transpose()?,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, ()| {
            let outcome =
                crate::download_store_observations::apply_download_failure(transaction, input)?;
            let DownloadObservationOutcome::Updated(record) = &outcome else {
                return Err(StorageError::DownloadWorkflowConflict);
            };
            if record.workflow_revision.value != expected.value.saturating_add(1) {
                return Err(StorageError::RevisionConflict);
            }
            super::download::retire_download_effects(transaction, record.episode_id)?;
            Ok(record.workflow_revision)
        },
    )?;
    let episode_id = episode
        .get()
        .ok_or(StorageError::DownloadWorkflowNotFound)?;
    Ok(DownloadObservationOutcome::Updated(
        crate::LibraryStore::open_authoritative(path)?
            .download_workflow(episode_id)?
            .ok_or(StorageError::DownloadWorkflowNotFound)?,
    ))
}

fn identity(
    episode_id: EpisodeId,
    revision: StateRevision,
    issued_revision: StateRevision,
    now_ms: i64,
    deadline_at_ms: Option<i64>,
) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/download-waiting-reconciliation/v1");
    hash.update(episode_id.into_bytes());
    hash.update(revision.value.to_be_bytes());
    hash.update(issued_revision.value.to_be_bytes());
    hash.update(now_ms.to_be_bytes());
    hash.update(deadline_at_ms.unwrap_or(-1).to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    CommandId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
}

fn fingerprint(
    episode_id: EpisodeId,
    expected: StateRevision,
    issued: StateRevision,
    now_ms: i64,
    deadline: Option<i64>,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/download-waiting-reconciliation-ingress/v1");
    hash.update(episode_id.into_bytes());
    hash.update(expected.value.to_be_bytes());
    hash.update(issued.value.to_be_bytes());
    hash.update(now_ms.to_be_bytes());
    hash.update(deadline.unwrap_or(-1).to_be_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}

fn timeout_identity(request_id: pod0_domain::HostRequestId, sequence: u64) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/download-timeout-recovery/v1");
    hash.update(request_id.into_bytes());
    hash.update(sequence.to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    CommandId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
}

fn timeout_fingerprint(input: &DownloadFailureInput) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/download-timeout-recovery-ingress/v1");
    hash.update(input.request_id.into_bytes());
    hash.update(input.sequence_number.to_be_bytes());
    hash.update(input.observed_at_ms.to_be_bytes());
    hash.update(input.retry_at_ms.unwrap_or(-1).to_be_bytes());
    hash.update(input.retry_deadline_at_ms.unwrap_or(-1).to_be_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}
