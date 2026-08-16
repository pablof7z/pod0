use pod0_application::{
    ChapterEffectObservationActivityInput, ChapterRecordedTransition, ChapterTransition,
    EffectOutcome, ExternalEffectKind, plan_chapter_effect_observation,
};
use pod0_domain::{ContentDigest, EffectAttemptId, EpisodeId, StateRevision};
use rusqlite::OptionalExtension;
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{
    EffectOutboxError, LibraryStore, ModelChapterObservationAction,
    ModelChapterObservationCommitInput, ModelChapterObservationCommitOutcome,
    ModelChapterWorkflowRecord, StorageError, TransitionIngress, TransitionIngressKind,
};

impl LibraryStore {
    pub fn commit_model_chapter_observation(
        &self,
        input: ModelChapterObservationCommitInput,
    ) -> Result<ModelChapterObservationCommitOutcome, StorageError> {
        commit(self.path(), input)
    }
}

fn commit(
    path: &std::path::Path,
    input: ModelChapterObservationCommitInput,
) -> Result<ModelChapterObservationCommitOutcome, StorageError> {
    let fingerprint = fingerprint(&input);
    let terminal = !matches!(
        input.action,
        ModelChapterObservationAction::ProviderAccepted(_)
    );
    let identity = observation_identity(&input);
    let staged = input.observation.clone();
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
            let current = workflow_for_observation(transaction, &input)?;
            let no_change = matches!(
                &input.action,
                ModelChapterObservationAction::ProviderAccepted(value)
                    if current.state == crate::ModelChapterWorkflowState::ProviderAccepted
                        && current.provider_operation_id.as_deref()
                            == Some(value.provider_operation_id.as_str())
                        && current.provider_status == value.provider_status
            );
            plan_chapter_effect_observation(ChapterEffectObservationActivityInput {
                identity_attempt_id: identity,
                request_id: input.observation.request_id,
                command_id: current.command_id,
                episode_id: current.episode_id,
                current_revision: current.workflow_revision,
                intent_id: input.lease.intent_id,
                attempt_id: input.lease.attempt_id,
                authorizing_activity_id: input.lease.authorizing_activity_id,
                correlation_id: input.lease.correlation_id,
                outcome,
                transitions: if no_change {
                    Vec::new()
                } else {
                    vec![ChapterRecordedTransition {
                        kind: ChapterTransition::ModelWorkflowStateChanged,
                        previous_revision: current.workflow_revision,
                        committed_revision: next_revision(current.workflow_revision)?,
                    }]
                },
                next_effect: None,
                authorize_finalization: matches!(
                    input.action,
                    ModelChapterObservationAction::Completion(_)
                ),
                effect_kind: ExternalEffectKind::ModelChapterProvider,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction| {
            if terminal {
                crate::effect_outbox::stage_model_chapter_observation_in_transaction(
                    transaction,
                    input.lease,
                    &staged,
                    fingerprint,
                    outcome,
                )
            } else {
                crate::effect_outbox::validate_model_chapter_observation_lease_in_transaction(
                    transaction,
                    input.lease,
                    &staged,
                )
            }
            .map_err(effect_error)
        },
        |transaction, expected, mutation| {
            let current = workflow_for_observation(transaction, &input)?;
            if current.workflow_revision != expected {
                return Err(StorageError::RevisionConflict);
            }
            let updated = apply_action(transaction, action)?;
            match mutation {
                pod0_application::ChapterObservationMutation::Apply
                    if updated.workflow_revision.value == expected.value.saturating_add(1) => {}
                pod0_application::ChapterObservationMutation::RecordNoChange
                    if updated.workflow_revision == expected => {}
                _ => return Err(StorageError::RevisionConflict),
            }
            Ok(updated.workflow_revision)
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
    let workflow = LibraryStore::open_authoritative(path)?
        .model_chapter_workflow(action_episode(&input.action))?
        .ok_or(StorageError::ChapterWorkflowNotFound)?;
    Ok(ModelChapterObservationCommitOutcome {
        workflow,
        replayed: receipt.replayed,
        terminal_effect: terminal,
    })
}

fn apply_action(
    transaction: &rusqlite::Transaction<'_>,
    action: ModelChapterObservationAction,
) -> Result<ModelChapterWorkflowRecord, StorageError> {
    match action {
        ModelChapterObservationAction::ProviderAccepted(input) => {
            LibraryStore::apply_model_chapter_provider_accepted(transaction, input)
        }
        ModelChapterObservationAction::Completion(input) => {
            let episode_id = input.episode_id;
            LibraryStore::apply_model_chapter_completion_stage(transaction, input)?;
            crate::model_chapter_workflow::read::read_workflow(transaction, episode_id)?
                .ok_or(StorageError::ChapterWorkflowNotFound)
        }
        ModelChapterObservationAction::Failure { input, .. } => {
            LibraryStore::apply_model_chapter_failure(transaction, input)
        }
    }
}

fn workflow_for_observation(
    transaction: &rusqlite::Transaction<'_>,
    input: &ModelChapterObservationCommitInput,
) -> Result<ModelChapterWorkflowRecord, StorageError> {
    let episode: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT episode_id FROM pod0_effect_intents WHERE intent_id=?1
             AND effect_kind_code=4 AND subject_code=2",
            [input.lease.intent_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read model chapter effect subject", error))?;
    let episode_id = EpisodeId::from_bytes(
        episode
            .ok_or(StorageError::ChapterWorkflowNotFound)?
            .try_into()
            .map_err(|_| StorageError::InvalidActivity)?,
    );
    if episode_id != action_episode(&input.action) {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    let workflow = crate::model_chapter_workflow::read::read_workflow(transaction, episode_id)?
        .ok_or(StorageError::ChapterWorkflowNotFound)?;
    if workflow.request_id != Some(input.observation.request_id)
        || workflow.cancellation_id != input.observation.cancellation_id
        || workflow.issued_revision != input.observation.observed_request_revision
    {
        return Err(StorageError::ChapterWorkflowConflict);
    }
    Ok(workflow)
}

fn effect_outcome(action: &ModelChapterObservationAction) -> EffectOutcome {
    match action {
        ModelChapterObservationAction::ProviderAccepted(_) => EffectOutcome::OutcomeUnknown,
        ModelChapterObservationAction::Completion(_) => EffectOutcome::Succeeded,
        ModelChapterObservationAction::Failure { outcome, .. } => *outcome,
    }
}

fn action_episode(action: &ModelChapterObservationAction) -> EpisodeId {
    match action {
        ModelChapterObservationAction::ProviderAccepted(input) => input.episode_id,
        ModelChapterObservationAction::Completion(input) => input.episode_id,
        ModelChapterObservationAction::Failure { input, .. } => input.episode_id,
    }
}

fn observation_identity(input: &ModelChapterObservationCommitInput) -> EffectAttemptId {
    if !matches!(
        input.action,
        ModelChapterObservationAction::ProviderAccepted(_)
    ) {
        return input.lease.attempt_id;
    }
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/model-stream-observation-id/v1");
    hash.update(input.lease.attempt_id.into_bytes());
    hash.update(input.observation.sequence_number.to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    EffectAttemptId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
}

fn fingerprint(input: &ModelChapterObservationCommitInput) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/model-observation/v1");
    hash.update(input.lease.intent_id.into_bytes());
    hash.update(input.lease.attempt_id.into_bytes());
    hash.update(input.observation.request_id.into_bytes());
    hash.update(input.observation.sequence_number.to_be_bytes());
    match &input.action {
        ModelChapterObservationAction::ProviderAccepted(value) => {
            hash.update([1]);
            hash_text(&mut hash, &value.provider_operation_id);
            hash_optional_text(&mut hash, value.provider_status.as_deref());
        }
        ModelChapterObservationAction::Completion(value) => {
            hash.update([2]);
            hash.update(Sha256::digest(value.completion.as_bytes()));
            hash_text(&mut hash, &value.provider);
            hash_text(&mut hash, &value.model);
            hash_optional_text(&mut hash, value.provider_operation_id.as_deref());
            hash_optional_text(&mut hash, value.provider_status.as_deref());
        }
        ModelChapterObservationAction::Failure { input, .. } => {
            hash.update([3]);
            hash_text(&mut hash, &input.failure_code);
            hash_optional_text(&mut hash, input.failure_detail.as_deref());
        }
    }
    ContentDigest::from_bytes(hash.finalize().into())
}

fn hash_text(hash: &mut Sha256, value: &str) {
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value.as_bytes());
}

fn hash_optional_text(hash: &mut Sha256, value: Option<&str>) {
    hash.update([u8::from(value.is_some())]);
    if let Some(value) = value {
        hash_text(hash, value);
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
            operation: "commit model chapter effect observation",
        },
    }
}
