use pod0_application::{
    ActivityFailureCode, DownloadEffectAuthorization, DownloadFinalizationAuthorization,
    DownloadObservationActivityInput, EffectOutcome, download_observation_identity,
    plan_download_observation,
};
use pod0_domain::EpisodeId;
use rusqlite::OptionalExtension;

use super::TransitionCommit;
use crate::{
    DownloadLeasedObservationAction, DownloadObservationCommitInput,
    DownloadObservationCommitOutcome, DownloadObservationOutcome, DownloadWorkflowRecord,
    EffectOutboxError, StorageError, TransitionIngress, TransitionIngressKind,
};

impl crate::LibraryStore {
    pub fn commit_download_observation(
        &self,
        input: DownloadObservationCommitInput,
    ) -> Result<DownloadObservationCommitOutcome, StorageError> {
        commit(self.path(), input)
    }
}

fn commit(
    path: &std::path::Path,
    input: DownloadObservationCommitInput,
) -> Result<DownloadObservationCommitOutcome, StorageError> {
    let fingerprint = super::download_observation_fingerprint::fingerprint(&input);
    let terminal = !matches!(
        input.action,
        DownloadLeasedObservationAction::Accepted { .. }
    );
    let identity =
        download_observation_identity(input.lease.attempt_id, input.observation.sequence_number);
    let staged = input.observation.clone();
    let mutation_observation = input.observation.clone();
    let action = input.action.clone();
    let outcome = effect_outcome(&input.action);
    let receipt = TransitionCommit::open(path)?.commit_planned_with_transaction_hooks(
        TransitionIngress {
            kind: TransitionIngressKind::HostObservation,
            id: identity.into_bytes(),
            fingerprint,
        },
        input.committed_at,
        |transaction| {
            let (host, current) = current(transaction, &input)?;
            plan_download_observation(DownloadObservationActivityInput {
                identity_attempt_id: identity,
                effect_attempt_id: input.lease.attempt_id,
                request_id: input.observation.request_id,
                command_id: current.command_id,
                episode_id: current.episode_id,
                current_revision: current.workflow_revision,
                intent_id: input.lease.intent_id,
                authorizing_activity_id: input.lease.authorizing_activity_id,
                correlation_id: input.lease.correlation_id,
                outcome,
                state_changes: !matches!(
                    input.action,
                    DownloadLeasedObservationAction::Cancellation
                ) || current.request_id == Some(host.request_id),
                next_effect: retry_effect(&input.action, &current)?,
                finalization: finalization(&input.action, input.observation.sequence_number),
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction| {
            if terminal {
                crate::effect_outbox::stage_download_observation_in_transaction(
                    transaction,
                    input.lease,
                    &staged,
                    fingerprint,
                    outcome,
                )
            } else {
                crate::effect_outbox::validate_download_observation_lease_in_transaction(
                    transaction,
                    input.lease,
                    &staged,
                )
            }
            .map_err(effect_error)
        },
        |transaction, expected, mutation| {
            let (_, current) = current(transaction, &input)?;
            if current.workflow_revision != expected {
                return Err(StorageError::RevisionConflict);
            }
            let updated = apply(transaction, action, &mutation_observation)?;
            match (mutation, updated) {
                (
                    pod0_application::DownloadObservationMutation::Apply,
                    DownloadObservationOutcome::Updated(record),
                ) if record.workflow_revision.value == expected.value.saturating_add(1) => {
                    Ok(record.workflow_revision)
                }
                (
                    pod0_application::DownloadObservationMutation::RecordNoChange,
                    DownloadObservationOutcome::Updated(record),
                ) if record.workflow_revision == expected => Ok(expected),
                _ => Err(StorageError::DownloadWorkflowConflict),
            }
        },
        |transaction| {
            if terminal {
                crate::effect_outbox::complete_host_observation_in_transaction(
                    transaction,
                    input.lease,
                )
                .map_err(effect_error)?;
            }
            Ok(())
        },
    )?;
    let episode_id = effect_episode(path, input.lease.intent_id)?;
    let workflow = crate::LibraryStore::open_authoritative(path)?
        .download_workflow(episode_id)?
        .ok_or(StorageError::DownloadWorkflowNotFound)?;
    Ok(DownloadObservationCommitOutcome {
        workflow,
        replayed: receipt.replayed,
        terminal_effect: terminal,
    })
}

fn apply(
    transaction: &rusqlite::Transaction<'_>,
    action: DownloadLeasedObservationAction,
    observation: &pod0_application::HostObservationEnvelope,
) -> Result<DownloadObservationOutcome, StorageError> {
    match action {
        DownloadLeasedObservationAction::Accepted {
            external_task_key,
            resume_key,
        } => crate::download_store_observations::apply_download_host_task(
            transaction,
            observation.request_id,
            observation.sequence_number,
            &external_task_key,
            resume_key.as_deref(),
            observation.observed_at.value,
        ),
        DownloadLeasedObservationAction::Cancellation => {
            crate::download_store_observations::apply_download_cancellation(
                transaction,
                observation.request_id,
                observation.sequence_number,
                observation.observed_at.value,
            )
        }
        DownloadLeasedObservationAction::Removal { artifact_key } => {
            crate::download_store_observations::apply_download_artifact_removal(
                transaction,
                observation.request_id,
                observation.sequence_number,
                &artifact_key,
                observation.observed_at.value,
            )
        }
        DownloadLeasedObservationAction::Staged { .. } => {
            crate::download_store_finalization::apply_download_finalization_queued(
                transaction,
                observation.request_id,
                observation.sequence_number,
                observation.observed_at.value,
            )
        }
        DownloadLeasedObservationAction::Failure(input) => {
            crate::download_store_observations::apply_download_failure(transaction, input)
        }
    }
}

fn current(
    transaction: &rusqlite::Transaction<'_>,
    input: &DownloadObservationCommitInput,
) -> Result<(crate::DownloadHostRequestRecord, DownloadWorkflowRecord), StorageError> {
    let episode: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT episode_id FROM pod0_effect_intents WHERE intent_id=?1 AND effect_kind_code=5",
            [input.lease.intent_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read download effect episode", error))?;
    let episode_id = EpisodeId::from_bytes(
        episode
            .ok_or(StorageError::DownloadWorkflowNotFound)?
            .try_into()
            .map_err(|_| StorageError::InvalidActivity)?,
    );
    let (host, state) =
        crate::download_store_read::request(transaction, input.observation.request_id)?
            .ok_or(StorageError::DownloadRequestNotFound)?;
    let workflow = crate::download_store_read::workflow(transaction, episode_id)?
        .ok_or(StorageError::DownloadWorkflowNotFound)?;
    if host.episode_id != episode_id
        || state != "pending"
        || host.cancellation_id != input.observation.cancellation_id
        || host.issued_revision != input.observation.observed_request_revision
    {
        return Err(StorageError::DownloadWorkflowConflict);
    }
    Ok((host, workflow))
}

fn retry_effect(
    action: &DownloadLeasedObservationAction,
    current: &DownloadWorkflowRecord,
) -> Result<Option<DownloadEffectAuthorization>, StorageError> {
    let DownloadLeasedObservationAction::Failure(input) = action else {
        return Ok(None);
    };
    (input.retryable && input.retry_at_ms.is_some() && input.retry_deadline_at_ms.is_some())
        .then(|| {
            crate::download_effect_request::retry(current, input)
                .map(|request| DownloadEffectAuthorization { request })
        })
        .transpose()
}

fn finalization(
    action: &DownloadLeasedObservationAction,
    sequence_number: u64,
) -> Option<DownloadFinalizationAuthorization> {
    let DownloadLeasedObservationAction::Staged {
        staged_file_path,
        claimed_byte_count,
    } = action
    else {
        return None;
    };
    Some(DownloadFinalizationAuthorization {
        staged_file_path: staged_file_path.clone(),
        claimed_byte_count: *claimed_byte_count,
        sequence_number,
    })
}

fn effect_outcome(action: &DownloadLeasedObservationAction) -> EffectOutcome {
    match action {
        DownloadLeasedObservationAction::Accepted { .. } => EffectOutcome::OutcomeUnknown,
        DownloadLeasedObservationAction::Cancellation => EffectOutcome::Cancelled,
        DownloadLeasedObservationAction::Removal { .. } => EffectOutcome::Succeeded,
        DownloadLeasedObservationAction::Staged { .. } => EffectOutcome::Succeeded,
        DownloadLeasedObservationAction::Failure(input) => EffectOutcome::Failed {
            code: failure_code(&input.failure_code),
        },
    }
}

fn failure_code(code: &str) -> ActivityFailureCode {
    match code {
        "offline" => ActivityFailureCode::Offline,
        "timed_out" => ActivityFailureCode::TimedOut,
        "permission_denied" => ActivityFailureCode::PermissionDenied,
        "host_rejected" => ActivityFailureCode::InvalidResponse,
        _ => ActivityFailureCode::PlatformFailure,
    }
}

fn effect_episode(
    path: &std::path::Path,
    intent_id: pod0_domain::EffectIntentId,
) -> Result<EpisodeId, StorageError> {
    crate::LibraryStore::open_authoritative(path)?.read(|connection| {
        let value: Vec<u8> = connection
            .query_row(
                "SELECT episode_id FROM pod0_effect_intents WHERE intent_id=?1 AND effect_kind_code=5",
                [intent_id.into_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(|error| StorageError::sqlite("read download effect result", error))?;
        Ok(EpisodeId::from_bytes(
            value.try_into().map_err(|_| StorageError::InvalidActivity)?,
        ))
    })
}

fn effect_error(error: EffectOutboxError) -> StorageError {
    match error {
        EffectOutboxError::StaleLease => StorageError::StaleDownloadAttempt,
        _ => StorageError::InvalidActivity,
    }
}
