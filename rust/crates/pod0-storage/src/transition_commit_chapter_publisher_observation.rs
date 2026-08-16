use pod0_application::{
    ChapterEffectObservationActivityInput, ChapterRecordedTransition, ChapterTransition,
    ChapterWorkflowEffectAuthorization, ChapterWorkflowExecution, EffectOutcome,
    RequestDisposition,
    plan_chapter_effect_observation,
};
use pod0_domain::{ChapterArtifact, EpisodeId, StateRevision};
use rusqlite::OptionalExtension;

#[path = "transition_commit_chapter_publisher_observation_fingerprint.rs"]
mod observation_fingerprint;

use super::TransitionCommit;
use crate::{
    EffectOutboxError, PublisherChapterObservationAction, PublisherChapterObservationCommitInput,
    PublisherChapterObservationCommitOutcome, PublisherChapterWorkflowRecord,
    PublisherChapterWorkflowUpdate, StorageError, TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_publisher_chapter_observation(
    path: &std::path::Path,
    input: PublisherChapterObservationCommitInput,
) -> Result<PublisherChapterObservationCommitOutcome, StorageError> {
    let fingerprint = observation_fingerprint::fingerprint(&input)?;
    let staged = input.observation.clone();
    let action = input.action.clone();
    let outcome = effect_outcome(&input.action);
    let receipt = TransitionCommit::open(path)?.commit_planned_with_transaction_hooks(
        TransitionIngress {
            kind: TransitionIngressKind::HostObservation,
            id: input.lease.attempt_id.into_bytes(),
            fingerprint,
        },
        input.committed_at,
        |transaction| {
            let current = workflow_for_observation(transaction, &input)?;
            let mut transitions = vec![ChapterRecordedTransition {
                kind: ChapterTransition::PublisherWorkflowStateChanged,
                previous_revision: current.workflow_revision,
                committed_revision: next_revision(current.workflow_revision)?,
            }];
            if let PublisherChapterObservationAction::Complete { artifact } = &input.action {
                let sealed = ChapterArtifact::seal(artifact.clone())
                    .map_err(|_| StorageError::InvalidChapterArtifact)?;
                let selected = crate::chapter_workflow_store_support::selected_chapter(
                    transaction,
                    current.episode_id,
                )?;
                if selected.map(|item| item.0) != Some(sealed.artifact_id) {
                    let actual = selected.map_or(StateRevision::INITIAL, |item| item.1);
                    if actual != current.expected_selection_revision {
                        return Err(StorageError::ChapterRevisionConflict);
                    }
                    let committed = next_revision(actual)?;
                    transitions.extend([
                        ChapterRecordedTransition {
                            kind: ChapterTransition::ArtifactAdopted,
                            previous_revision: actual,
                            committed_revision: committed,
                        },
                        ChapterRecordedTransition {
                            kind: ChapterTransition::SelectionChanged,
                            previous_revision: actual,
                            committed_revision: committed,
                        },
                    ]);
                }
            }
            let next_effect = retry_effect(&input.action, &current)?;
            plan_chapter_effect_observation(ChapterEffectObservationActivityInput {
                identity_attempt_id: input.lease.attempt_id,
                request_id: input.observation.request_id,
                command_id: current.command_id,
                episode_id: current.episode_id,
                current_revision: current.workflow_revision,
                intent_id: input.lease.intent_id,
                attempt_id: input.lease.attempt_id,
                authorizing_activity_id: input.lease.authorizing_activity_id,
                correlation_id: input.lease.correlation_id,
                outcome,
                transitions,
                next_effect,
                authorize_finalization: false,
                effect_kind: pod0_application::ExternalEffectKind::PublisherChapterProvider,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction| {
            crate::effect_outbox::stage_chapter_observation_in_transaction(
                transaction,
                input.lease,
                &staged,
                fingerprint,
                outcome,
            )
            .map_err(effect_error)
        },
        |transaction, expected, mutation| {
            if mutation != pod0_application::ChapterObservationMutation::Apply {
                return Err(StorageError::InvalidActivity);
            }
            let current = workflow_for_observation(transaction, &input)?;
            if current.workflow_revision != expected {
                return Err(StorageError::RevisionConflict);
            }
            let updated = apply_action(transaction, current, action, input.committed_at.value)?;
            Ok(updated.workflow_revision)
        },
        |transaction| {
            crate::effect_outbox::complete_host_observation_in_transaction(transaction, input.lease)
                .map_err(effect_error)
        },
    )?;
    let workflow = crate::LibraryStore::open_authoritative(path)?
        .publisher_chapter_workflow(effect_episode_id(path, input.lease.intent_id)?)?
        .ok_or(StorageError::ChapterWorkflowNotFound)?;
    if receipt.disposition != RequestDisposition::Accepted {
        return Err(StorageError::InvalidActivity);
    }
    Ok(PublisherChapterObservationCommitOutcome {
        workflow,
        replayed: receipt.replayed,
    })
}

fn apply_action(
    transaction: &rusqlite::Transaction<'_>,
    current: PublisherChapterWorkflowRecord,
    action: PublisherChapterObservationAction,
    committed_at_ms: i64,
) -> Result<PublisherChapterWorkflowRecord, StorageError> {
    match action {
        PublisherChapterObservationAction::Complete { artifact } => {
            crate::LibraryStore::apply_publisher_chapter_completion(
                transaction,
                current
                    .request_id
                    .ok_or(StorageError::ChapterWorkflowConflict)?,
                artifact,
                committed_at_ms,
            )
        }
        PublisherChapterObservationAction::Fail { failure, .. } => {
            match crate::LibraryStore::apply_publisher_chapter_failure(transaction, failure)? {
                PublisherChapterWorkflowUpdate::RetryScheduled(record)
                | PublisherChapterWorkflowUpdate::Failed(record) => Ok(record),
            }
        }
        PublisherChapterObservationAction::Cancel
        | PublisherChapterObservationAction::Supersede => {
            crate::LibraryStore::apply_publisher_chapter_cancellation(
                transaction,
                current.episode_id,
                current.workflow_revision,
                committed_at_ms,
            )
        }
    }
}

fn retry_effect(
    action: &PublisherChapterObservationAction,
    current: &PublisherChapterWorkflowRecord,
) -> Result<Option<ChapterWorkflowEffectAuthorization>, StorageError> {
    let PublisherChapterObservationAction::Fail { failure, .. } = action else {
        return Ok(None);
    };
    if failure.retry_at_ms.is_none() || current.attempt >= current.max_attempts {
        return Ok(None);
    }
    let generation = current
        .generation
        .checked_add(1)
        .ok_or(StorageError::ChapterWorkflowConflict)?;
    let request_id = crate::chapter_workflow_store_support::request_id_for_generation(
        current.episode_id,
        &current.source_version,
        generation,
    );
    Ok(Some(ChapterWorkflowEffectAuthorization {
            not_before: failure
                .retry_at_ms
                .map(pod0_domain::UnixTimestampMilliseconds::new),
            deadline_at: failure
                .retry_deadline_at_ms
                .map(pod0_domain::UnixTimestampMilliseconds::new),
            execution: ChapterWorkflowExecution::Publisher(
                crate::chapter_effect_request::publisher_request(
                    request_id,
                    current.command_id,
                    current.cancellation_id,
                    failure.retry_issued_revision,
                    failure
                        .retry_deadline_at_ms
                        .map(pod0_domain::UnixTimestampMilliseconds::new),
                    current.episode_id,
                    current.source_url.clone(),
                    failure
                        .retry_at_ms
                        .map(pod0_domain::UnixTimestampMilliseconds::new),
                ),
            ),
        }))
}

fn workflow_for_observation(
    transaction: &rusqlite::Transaction<'_>,
    input: &PublisherChapterObservationCommitInput,
) -> Result<PublisherChapterWorkflowRecord, StorageError> {
    let episode: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT episode_id FROM pod0_effect_intents WHERE intent_id=?1
             AND effect_kind_code=4 AND subject_code=2",
            [input.lease.intent_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read publisher chapter effect subject", error))?;
    let episode_id = EpisodeId::from_bytes(
        episode
            .ok_or(StorageError::ChapterWorkflowNotFound)?
            .try_into()
            .map_err(|_| StorageError::InvalidActivity)?,
    );
    let workflow = crate::chapter_workflow_store_read::read_workflow(transaction, episode_id)?
        .ok_or(StorageError::ChapterWorkflowNotFound)?;
    if workflow.request_id != Some(input.observation.request_id)
        || workflow.cancellation_id != input.observation.cancellation_id
        || workflow.issued_revision != input.observation.observed_request_revision
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    Ok(workflow)
}

fn effect_episode_id(
    path: &std::path::Path,
    intent_id: pod0_domain::EffectIntentId,
) -> Result<EpisodeId, StorageError> {
    crate::LibraryStore::open_authoritative(path)?.read(|connection| {
        let bytes: Vec<u8> = connection
            .query_row(
                "SELECT episode_id FROM pod0_effect_intents WHERE intent_id=?1
                 AND effect_kind_code=4 AND subject_code=2",
                [intent_id.into_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(|error| StorageError::sqlite("read publisher effect episode", error))?;
        Ok(EpisodeId::from_bytes(
            bytes
                .try_into()
                .map_err(|_| StorageError::InvalidActivity)?,
        ))
    })
}

fn effect_outcome(action: &PublisherChapterObservationAction) -> EffectOutcome {
    match action {
        PublisherChapterObservationAction::Complete { .. } => EffectOutcome::Succeeded,
        PublisherChapterObservationAction::Fail { outcome_code, .. } => EffectOutcome::Failed {
            code: *outcome_code,
        },
        PublisherChapterObservationAction::Cancel => EffectOutcome::Cancelled,
        PublisherChapterObservationAction::Supersede => EffectOutcome::Superseded,
    }
}

fn next_revision(revision: StateRevision) -> Result<StateRevision, StorageError> {
    revision
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(StorageError::InvalidActivity)
}

fn effect_error(error: EffectOutboxError) -> StorageError {
    match error {
        EffectOutboxError::StaleLease => StorageError::ChapterWorkflowConflict,
        EffectOutboxError::InvalidRecord | EffectOutboxError::InvalidLeaseDuration => {
            StorageError::InvalidActivity
        }
        EffectOutboxError::Storage => StorageError::Sqlite {
            operation: "commit publisher chapter effect observation",
        },
    }
}
