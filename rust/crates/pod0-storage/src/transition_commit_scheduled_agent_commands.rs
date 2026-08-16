use pod0_application::{
    ActivitySubject, ScheduledAgentActivityTransition, ScheduledCommandActivityInput,
    ScheduledTaskDefinition, plan_scheduled_command, validate_scheduled_task_definition,
};
use pod0_domain::{ScheduledTaskId, StateRevision};

use super::TransitionCommit;
use super::scheduled_agent_effects::{affected_task_work, supersede_effects};
use crate::scheduled_agent_store_read::read_task;
use crate::scheduled_agent_store_tasks::{
    command_receipt, core_revision, ensure_task_in_transaction, remove_task_in_transaction,
    update_task_in_transaction, validate_context,
};
use crate::{
    ScheduledAgentCommandContext, ScheduledTaskMutationOutcome, ScheduledTaskRemovalOutcome,
    StorageError, TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_scheduled_task_ensure(
    path: &std::path::Path,
    context: ScheduledAgentCommandContext,
    definition: ScheduledTaskDefinition,
) -> Result<ScheduledTaskMutationOutcome, StorageError> {
    validate_context(&context)?;
    validate_scheduled_task_definition(&definition)
        .map_err(|_| StorageError::ScheduledAgentWorkflowConflict)?;
    if definition.revision != StateRevision::new(1)
        || definition.created_at != context.observed_at
        || definition.last_run_at.is_some()
    {
        return Err(StorageError::ScheduledAgentWorkflowConflict);
    }
    let output = std::cell::RefCell::new(None);
    let candidate = definition.clone();
    let receipt = commit_command(
        path,
        context,
        |transaction, current| {
            let duplicate = command_receipt(transaction, &context)?.is_some();
            let existing = read_task(transaction, definition.task_id, false)?;
            if let Some(existing) = &existing
                && existing != &definition
            {
                return Err(StorageError::ScheduledAgentWorkflowConflict);
            }
            let disposition = if duplicate {
                pod0_application::RequestDisposition::Duplicate
            } else {
                pod0_application::RequestDisposition::Accepted
            };
            let changed = !duplicate;
            command_plan(
                context,
                current,
                disposition,
                changed
                    .then_some((
                        ActivitySubject::Global,
                        ScheduledAgentActivityTransition::TaskChanged,
                    ))
                    .into_iter()
                    .collect(),
                Vec::new(),
            )
        },
        |transaction| {
            let value = ensure_task_in_transaction(transaction, &context, candidate)?;
            *output.borrow_mut() = Some(value);
            Ok(())
        },
    )?;
    if receipt.replayed {
        let store = crate::ScheduledAgentStore::open_authoritative(path)?;
        let task = store
            .task(definition.task_id)?
            .ok_or(StorageError::ScheduledAgentTaskNotFound)?;
        Ok(ScheduledTaskMutationOutcome::Duplicate(task))
    } else {
        output.into_inner().ok_or(StorageError::InvalidActivity)
    }
}

pub(crate) fn commit_scheduled_task_update(
    path: &std::path::Path,
    context: ScheduledAgentCommandContext,
    expected_revision: StateRevision,
    definition: ScheduledTaskDefinition,
) -> Result<ScheduledTaskMutationOutcome, StorageError> {
    validate_context(&context)?;
    validate_scheduled_task_definition(&definition)
        .map_err(|_| StorageError::ScheduledAgentWorkflowConflict)?;
    if definition.revision.value != expected_revision.value.saturating_add(1) {
        return Err(StorageError::ScheduledAgentWorkflowConflict);
    }
    let output = std::cell::RefCell::new(None);
    let candidate = definition.clone();
    let receipt = commit_command(
        path,
        context,
        |transaction, current| {
            let duplicate = command_receipt(transaction, &context)?.is_some();
            command_plan(
                context,
                current,
                if duplicate {
                    pod0_application::RequestDisposition::Duplicate
                } else {
                    pod0_application::RequestDisposition::Accepted
                },
                (!duplicate)
                    .then_some((
                        ActivitySubject::Global,
                        ScheduledAgentActivityTransition::TaskChanged,
                    ))
                    .into_iter()
                    .collect(),
                Vec::new(),
            )
        },
        |transaction| {
            let value =
                update_task_in_transaction(transaction, &context, expected_revision, candidate)?;
            *output.borrow_mut() = Some(value);
            Ok(())
        },
    )?;
    if receipt.replayed {
        let task = crate::ScheduledAgentStore::open_authoritative(path)?
            .task(definition.task_id)?
            .ok_or(StorageError::ScheduledAgentTaskNotFound)?;
        Ok(ScheduledTaskMutationOutcome::Duplicate(task))
    } else {
        output.into_inner().ok_or(StorageError::InvalidActivity)
    }
}

pub(crate) fn commit_scheduled_task_remove(
    path: &std::path::Path,
    context: ScheduledAgentCommandContext,
    task_id: ScheduledTaskId,
    expected_revision: StateRevision,
) -> Result<ScheduledTaskRemovalOutcome, StorageError> {
    validate_context(&context)?;
    let output = std::cell::RefCell::new(None);
    let effects = std::cell::RefCell::new(Vec::new());
    let receipt = commit_command(
        path,
        context,
        |transaction, current| {
            let duplicate = command_receipt(transaction, &context)?.is_some();
            let affected = (!duplicate)
                .then(|| affected_task_work(transaction, task_id))
                .transpose()?
                .unwrap_or_default();
            *effects.borrow_mut() = affected.effects.clone();
            let mut transitions = Vec::new();
            if !duplicate {
                transitions.push((
                    ActivitySubject::Global,
                    ScheduledAgentActivityTransition::TaskChanged,
                ));
                for occurrence in &affected.occurrences {
                    let subject = ActivitySubject::ScheduledOccurrence {
                        occurrence_id: occurrence.occurrence_id,
                    };
                    transitions.push((
                        subject,
                        ScheduledAgentActivityTransition::OccurrenceStateChanged,
                    ));
                    if occurrence.has_attempt {
                        transitions.push((
                            subject,
                            ScheduledAgentActivityTransition::AttemptStateChanged,
                        ));
                    }
                }
            }
            command_plan(
                context,
                current,
                if duplicate {
                    pod0_application::RequestDisposition::Duplicate
                } else {
                    pod0_application::RequestDisposition::Accepted
                },
                transitions,
                affected
                    .effects
                    .iter()
                    .map(|effect| effect.subject)
                    .collect(),
            )
        },
        |transaction| {
            let value =
                remove_task_in_transaction(transaction, &context, task_id, expected_revision)?;
            supersede_effects(transaction, &effects.borrow())?;
            *output.borrow_mut() = Some(value);
            Ok(())
        },
    )?;
    if receipt.replayed {
        Ok(ScheduledTaskRemovalOutcome::Duplicate {
            task_id,
            revision: receipt.committed_revision,
        })
    } else {
        output.into_inner().ok_or(StorageError::InvalidActivity)
    }
}

fn command_plan(
    context: ScheduledAgentCommandContext,
    current: StateRevision,
    disposition: pod0_application::RequestDisposition,
    transitions: Vec<(ActivitySubject, ScheduledAgentActivityTransition)>,
    superseded_effects: Vec<ActivitySubject>,
) -> Result<pod0_application::ScheduledCommandPlan, StorageError> {
    let changed = !transitions.is_empty();
    plan_scheduled_command(ScheduledCommandActivityInput {
        command_id: context.command_id,
        current_revision: current,
        committed_revision: if changed {
            StateRevision::new(
                current
                    .value
                    .checked_add(1)
                    .ok_or(StorageError::InvalidActivity)?,
            )
        } else {
            current
        },
        disposition,
        transitions,
        effects: Vec::new(),
        superseded_effects,
    })
    .map_err(|_| StorageError::InvalidActivity)
}

pub(super) fn commit_command(
    path: &std::path::Path,
    context: ScheduledAgentCommandContext,
    plan: impl FnOnce(
        &rusqlite::Transaction<'_>,
        StateRevision,
    ) -> Result<pod0_application::ScheduledCommandPlan, StorageError>,
    mutate: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<(), StorageError>,
) -> Result<crate::transition_commit_model::CommitReceipt, StorageError> {
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: context.command_id.into_bytes(),
            fingerprint: pod0_domain::ContentDigest::from_bytes(context.command_fingerprint),
        },
        context.observed_at,
        |transaction| {
            let current = core_revision(transaction)?;
            plan(transaction, current)
        },
        |transaction, expected, _| {
            mutate(transaction)?;
            let committed = core_revision(transaction)?;
            if committed.value < expected.value {
                return Err(StorageError::InvalidActivity);
            }
            Ok(committed)
        },
    )
}
