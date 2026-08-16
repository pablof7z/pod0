use pod0_application::{
    ActivitySubject, RequestDisposition, RequestRejectionReason, ScheduledAgentActivityTransition,
    ScheduledAgentOccurrenceState, ScheduledAgentTransition, ScheduledCommandActivityInput,
    cancel_scheduled_agent, plan_scheduled_command, retry_scheduled_agent,
};
use pod0_domain::{ScheduledOccurrenceId, StateRevision};
use rusqlite::params;

use super::scheduled_agent_commands::commit_command;
use super::scheduled_agent_effects::{
    ActiveScheduledEffect, active_occurrence_effects, supersede_effects,
};
use crate::scheduled_agent_store_read::read_occurrence;
use crate::scheduled_agent_store_reconcile::persist_occurrence_state;
use crate::scheduled_agent_store_tasks::{command_receipt, finish_command};
use crate::{ScheduledAgentCommandContext, StorageError};

#[derive(Clone, Copy)]
enum ActionKind {
    Cancel,
    Retry,
}

#[derive(Clone)]
enum ActionDecision {
    Apply {
        previous: ScheduledAgentOccurrenceState,
        next: ScheduledAgentOccurrenceState,
        effects: Vec<ActiveScheduledEffect>,
    },
    Existing(ScheduledAgentOccurrenceState),
    Missing,
    Conflict,
    NotAllowed,
}

pub(crate) fn commit_scheduled_occurrence_cancel(
    path: &std::path::Path,
    context: ScheduledAgentCommandContext,
    occurrence_id: ScheduledOccurrenceId,
    expected_revision: StateRevision,
) -> Result<ScheduledAgentOccurrenceState, StorageError> {
    commit_action(
        path,
        context,
        occurrence_id,
        expected_revision,
        ActionKind::Cancel,
    )
}

pub(crate) fn commit_scheduled_occurrence_retry(
    path: &std::path::Path,
    context: ScheduledAgentCommandContext,
    occurrence_id: ScheduledOccurrenceId,
    expected_revision: StateRevision,
) -> Result<ScheduledAgentOccurrenceState, StorageError> {
    commit_action(
        path,
        context,
        occurrence_id,
        expected_revision,
        ActionKind::Retry,
    )
}

fn commit_action(
    path: &std::path::Path,
    context: ScheduledAgentCommandContext,
    occurrence_id: ScheduledOccurrenceId,
    expected_revision: StateRevision,
    kind: ActionKind,
) -> Result<ScheduledAgentOccurrenceState, StorageError> {
    let decision = std::cell::RefCell::new(None);
    let receipt = commit_command(
        path,
        context,
        |transaction, current| {
            let planned = decide(
                transaction,
                &context,
                occurrence_id,
                expected_revision,
                kind,
            )?;
            let (disposition, transitions, superseded_effects) = activity(&planned, kind);
            let changed = matches!(planned, ActionDecision::Apply { .. });
            *decision.borrow_mut() = Some(planned);
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
        },
        |transaction| {
            let Some(ActionDecision::Apply {
                previous,
                next,
                effects,
            }) = decision.borrow().as_ref().cloned()
            else {
                return Ok(());
            };
            persist_occurrence_state(transaction, &previous, &next)?;
            if matches!(kind, ActionKind::Cancel) {
                cancel_attempt(transaction, &next, context.observed_at.value())?;
                supersede_effects(transaction, &effects)?;
            }
            finish_command(
                transaction,
                &context,
                Some(next.task_id),
                Some(occurrence_id),
            )?;
            Ok(())
        },
    )?;
    match receipt.disposition {
        RequestDisposition::Accepted
        | RequestDisposition::Duplicate
        | RequestDisposition::AlreadyComplete => {
            if let Some(state) = decision.borrow().as_ref().and_then(decision_state) {
                return Ok(state);
            }
            crate::ScheduledAgentStore::open_authoritative(path)?
                .occurrence(occurrence_id)?
                .ok_or(StorageError::ScheduledAgentWorkflowNotFound)
        }
        RequestDisposition::Rejected {
            reason: RequestRejectionReason::MissingSubject,
        } => Err(StorageError::ScheduledAgentWorkflowNotFound),
        _ => Err(StorageError::ScheduledAgentWorkflowConflict),
    }
}

fn decide(
    transaction: &rusqlite::Transaction<'_>,
    context: &ScheduledAgentCommandContext,
    occurrence_id: ScheduledOccurrenceId,
    expected_revision: StateRevision,
    kind: ActionKind,
) -> Result<ActionDecision, StorageError> {
    if let Some(receipt) = command_receipt(transaction, context)? {
        let stored = receipt
            .occurrence_id
            .ok_or(StorageError::ScheduledAgentCommandConflict)?;
        let state = read_occurrence(transaction, stored)?
            .ok_or(StorageError::ScheduledAgentWorkflowNotFound)?;
        return Ok(if stored == occurrence_id {
            ActionDecision::Existing(state)
        } else {
            ActionDecision::Conflict
        });
    }
    let Some(previous) = read_occurrence(transaction, occurrence_id)? else {
        return Ok(ActionDecision::Missing);
    };
    if previous.revision != expected_revision {
        return Ok(ActionDecision::Conflict);
    }
    let mut next = previous.clone();
    let transition = match kind {
        ActionKind::Cancel => cancel_scheduled_agent(&mut next, context.observed_at),
        ActionKind::Retry => retry_scheduled_agent(&mut next, context.observed_at),
    };
    match transition {
        ScheduledAgentTransition::Applied => Ok(ActionDecision::Apply {
            previous,
            next,
            effects: if matches!(kind, ActionKind::Cancel) {
                active_occurrence_effects(transaction, occurrence_id)?
            } else {
                Vec::new()
            },
        }),
        ScheduledAgentTransition::IgnoredDuplicate => Ok(ActionDecision::Existing(previous)),
        ScheduledAgentTransition::IgnoredStale | ScheduledAgentTransition::RejectedInvalid => {
            Ok(ActionDecision::NotAllowed)
        }
    }
}

fn activity(
    decision: &ActionDecision,
    kind: ActionKind,
) -> (
    RequestDisposition,
    Vec<(ActivitySubject, ScheduledAgentActivityTransition)>,
    Vec<ActivitySubject>,
) {
    match decision {
        ActionDecision::Apply { next, effects, .. } => {
            let subject = ActivitySubject::ScheduledOccurrence {
                occurrence_id: next.occurrence_id,
            };
            let mut transitions = vec![(
                subject,
                ScheduledAgentActivityTransition::OccurrenceStateChanged,
            )];
            if matches!(kind, ActionKind::Cancel) && next.attempt_id.is_some() {
                transitions.push((
                    subject,
                    ScheduledAgentActivityTransition::AttemptStateChanged,
                ));
            }
            (
                RequestDisposition::Accepted,
                transitions,
                effects.iter().map(|effect| effect.subject).collect(),
            )
        }
        ActionDecision::Existing(_) => {
            (RequestDisposition::AlreadyComplete, Vec::new(), Vec::new())
        }
        ActionDecision::Missing => (
            RequestDisposition::Rejected {
                reason: RequestRejectionReason::MissingSubject,
            },
            Vec::new(),
            Vec::new(),
        ),
        ActionDecision::Conflict => (
            RequestDisposition::Rejected {
                reason: RequestRejectionReason::RevisionConflict,
            },
            Vec::new(),
            Vec::new(),
        ),
        ActionDecision::NotAllowed => (
            RequestDisposition::Rejected {
                reason: RequestRejectionReason::NotAllowed,
            },
            Vec::new(),
            Vec::new(),
        ),
    }
}

fn decision_state(decision: &ActionDecision) -> Option<ScheduledAgentOccurrenceState> {
    match decision {
        ActionDecision::Apply { next, .. } | ActionDecision::Existing(next) => Some(next.clone()),
        ActionDecision::Missing | ActionDecision::Conflict | ActionDecision::NotAllowed => None,
    }
}

fn cancel_attempt(
    transaction: &rusqlite::Transaction<'_>,
    state: &ScheduledAgentOccurrenceState,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let Some(attempt_id) = state.attempt_id else {
        return Ok(());
    };
    transaction
        .execute(
            "UPDATE pod0_scheduled_attempts SET state='cancelled',failure_code='cancelled',\
         failure_wire_code=NULL,failure_detail=NULL,failure_retryable=0,updated_at_ms=?1 \
         WHERE attempt_id=?2 AND state NOT IN('succeeded','failed','cancelled','ambiguous')",
            params![observed_at_ms, attempt_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("cancel scheduled attempt", error))?;
    Ok(())
}
