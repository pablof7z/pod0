use std::cell::RefCell;

use pod0_application::{
    ActivityDomain, ActivitySubject, ChapterTransition, DomainTransitionKind, InternalCommandKind,
    InternalCommandOwnerActivityInput, RequestDisposition, plan_internal_command_owner_activity,
};
use pod0_domain::{CommandId, StateRevision, UnixTimestampMilliseconds};

use super::TransitionCommit;
use crate::{
    ModelChapterEnsureInput, ModelChapterEnsureOutcome, PendingInternalCommand, StorageError,
    TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_model_chapter_internal_admission(
    path: &std::path::Path,
    command: PendingInternalCommand,
    mut input: ModelChapterEnsureInput,
) -> Result<ModelChapterEnsureOutcome, StorageError> {
    validate_command(&command, &input)?;
    input.command_id = CommandId::from_bytes(command.internal_command_id.into_bytes());
    let outcome = RefCell::new(None);
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::InternalCommand,
            id: command.internal_command_id.into_bytes(),
            fingerprint: super::chapter_model_admission::fingerprint(&input),
        },
        UnixTimestampMilliseconds::new(input.now_ms),
        |transaction| {
            let (current, changes) =
                crate::model_chapter_workflow::model_chapter_admission_state(transaction, &input)?;
            plan_internal_command_owner_activity(InternalCommandOwnerActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                command_id: input.command_id,
                subject: ActivitySubject::Episode {
                    episode_id: input.episode_id,
                },
                episode_id: Some(input.episode_id),
                current_revision: current,
                committed_revision: StateRevision::new(current.value + u64::from(changes)),
                disposition: if changes {
                    RequestDisposition::Accepted
                } else {
                    RequestDisposition::NoSemanticChange
                },
                transitions: changes
                    .then_some((
                        ActivitySubject::Episode {
                            episode_id: input.episode_id,
                        },
                        DomainTransitionKind::Chapter(ChapterTransition::ModelWorkflowStateChanged),
                    ))
                    .into_iter()
                    .collect(),
                effects: Vec::new(),
                internal_commands: Vec::new(),
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| {
            let applied =
                crate::model_chapter_workflow::apply_model_chapter_ensure(transaction, &input)?;
            let revision = outcome_revision(&applied);
            if mutation.changes_state {
                if revision.value != expected.value.saturating_add(1) {
                    return Err(StorageError::RevisionConflict);
                }
            } else if revision != expected {
                return Err(StorageError::RevisionConflict);
            }
            *outcome.borrow_mut() = Some(applied);
            Ok(revision)
        },
    )?;
    if receipt.replayed {
        let record = crate::LibraryStore::open_authoritative(path)?
            .model_chapter_workflow(input.episode_id)?
            .ok_or(StorageError::ChapterWorkflowNotFound)?;
        return Ok(ModelChapterEnsureOutcome::Existing(record));
    }
    outcome.into_inner().ok_or(StorageError::InvalidActivity)
}

fn validate_command(
    command: &PendingInternalCommand,
    input: &ModelChapterEnsureInput,
) -> Result<(), StorageError> {
    let InternalCommandKind::EnsureModelChapters { configured_model } = &command.request.kind
    else {
        return Err(StorageError::InvalidActivity);
    };
    if command.request.target != ActivityDomain::Chapter
        || command.request.episode_id != Some(input.episode_id)
        || configured_model != &input.configured_model
    {
        return Err(StorageError::InvalidActivity);
    }
    Ok(())
}

fn outcome_revision(outcome: &ModelChapterEnsureOutcome) -> StateRevision {
    match outcome {
        ModelChapterEnsureOutcome::Changed { record, .. }
        | ModelChapterEnsureOutcome::Existing(record) => record.workflow_revision,
    }
}
