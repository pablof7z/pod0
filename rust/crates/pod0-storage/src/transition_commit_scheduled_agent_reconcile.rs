use pod0_application::{
    ActivitySubject, ScheduledAgentActivityTransition, ScheduledCommandActivityInput,
    ScheduledEffectAuthorization, begin_scheduled_agent_attempt, plan_scheduled_command,
    reconcile_scheduled_occurrence,
};

use super::TransitionCommit;
use crate::scheduled_agent_store_read::{active_tasks, pending_requests, read_occurrence};
use crate::scheduled_agent_store_reconcile::{
    insert_occurrence, persist_attempt, retry_candidates,
};
use crate::scheduled_agent_store_tasks::{command_receipt, core_revision, finish_command};
use crate::{
    ScheduledAgentCommandContext, ScheduledAgentReconcileOutcome, StorageError, TransitionIngress,
    TransitionIngressKind,
};

#[derive(Clone)]
pub(super) struct ReconcilePlan {
    pub(super) new_occurrences: Vec<(
        pod0_application::ScheduledTaskDefinition,
        pod0_application::ScheduledAgentOccurrenceState,
    )>,
    pub(super) attempts: Vec<(
        pod0_application::ScheduledAgentOccurrenceState,
        pod0_application::ScheduledAgentAttemptPlan,
    )>,
}

pub(crate) fn commit_scheduled_agent_reconcile(
    path: &std::path::Path,
    context: ScheduledAgentCommandContext,
) -> Result<ScheduledAgentReconcileOutcome, StorageError> {
    let planned = std::cell::RefCell::new(None::<ReconcilePlan>);
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::ScheduledWake,
            id: context.command_id.into_bytes(),
            fingerprint: pod0_domain::ContentDigest::from_bytes(context.command_fingerprint),
        },
        context.observed_at,
        |transaction| {
            let current = core_revision(transaction)?;
            if command_receipt(transaction, &context)?.is_some() {
                return plan_scheduled_command(ScheduledCommandActivityInput {
                    command_id: context.command_id,
                    current_revision: current,
                    committed_revision: current,
                    disposition: pod0_application::RequestDisposition::Duplicate,
                    transitions: Vec::new(),
                    effects: Vec::new(),
                    superseded_effects: Vec::new(),
                })
                .map_err(|_| StorageError::InvalidActivity);
            }
            let plan = build_plan(transaction, &context)?;
            let mut transitions = plan
                .new_occurrences
                .iter()
                .map(|(_, state)| {
                    (
                        ActivitySubject::ScheduledOccurrence {
                            occurrence_id: state.occurrence_id,
                        },
                        ScheduledAgentActivityTransition::OccurrenceStateChanged,
                    )
                })
                .collect::<Vec<_>>();
            transitions.extend(plan.attempts.iter().map(|(_, attempt)| {
                (
                    ActivitySubject::ScheduledOccurrence {
                        occurrence_id: attempt.state.occurrence_id,
                    },
                    ScheduledAgentActivityTransition::AttemptStateChanged,
                )
            }));
            let effects = plan
                .attempts
                .iter()
                .map(|(_, attempt)| ScheduledEffectAuthorization {
                    request: pod0_application::DurableScheduledAgentEffectRequest {
                        request_id: attempt.request_id,
                        command_id: context.command_id,
                        cancellation_id: context.cancellation_id,
                        issued_revision: context.issued_revision,
                        deadline_at: attempt.deadline_at,
                        execution: attempt.request.clone(),
                    },
                })
                .collect();
            *planned.borrow_mut() = Some(plan);
            plan_scheduled_command(ScheduledCommandActivityInput {
                command_id: context.command_id,
                current_revision: current,
                committed_revision: pod0_domain::StateRevision::new(
                    current
                        .value
                        .checked_add(1)
                        .ok_or(StorageError::InvalidActivity)?,
                ),
                disposition: pod0_application::RequestDisposition::Accepted,
                transitions,
                effects,
                superseded_effects: Vec::new(),
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, _| {
            let Some(plan) = planned.borrow().as_ref().cloned() else {
                return Ok(expected);
            };
            for (definition, occurrence) in &plan.new_occurrences {
                insert_occurrence(transaction, definition, occurrence)?;
            }
            for (previous, attempt) in &plan.attempts {
                persist_attempt(transaction, &context, previous, attempt)?;
            }
            finish_command(transaction, &context, None, None)
        },
    )?;
    let store = crate::ScheduledAgentStore::open_authoritative(path)?;
    Ok(ScheduledAgentReconcileOutcome {
        created_occurrences: planned
            .borrow()
            .as_ref()
            .map(|plan| {
                plan.new_occurrences
                    .iter()
                    .map(|(_, state)| state.occurrence_id)
                    .collect()
            })
            .unwrap_or_default(),
        requests: store.read(|connection| {
            pending_requests(
                connection,
                (!receipt.replayed).then_some(context.command_id),
                u16::MAX,
            )
        })?,
    })
}

pub(super) fn build_plan(
    transaction: &rusqlite::Transaction<'_>,
    context: &ScheduledAgentCommandContext,
) -> Result<ReconcilePlan, StorageError> {
    let mut new_occurrences = Vec::new();
    for definition in active_tasks(transaction)? {
        let Some(occurrence) = reconcile_scheduled_occurrence(&definition, context.observed_at)
            .map_err(|_| StorageError::ScheduledAgentWorkflowConflict)?
        else {
            continue;
        };
        if read_occurrence(transaction, occurrence.occurrence_id)?.is_none() {
            new_occurrences.push((definition, occurrence));
        }
    }
    let mut candidates = retry_candidates(transaction, context.observed_at.value())?;
    candidates.extend(
        new_occurrences
            .iter()
            .map(|(_, occurrence)| occurrence.occurrence_id),
    );
    let mut attempts = Vec::with_capacity(candidates.len());
    for occurrence_id in candidates {
        let occurrence = new_occurrences
            .iter()
            .find_map(|(_, state)| (state.occurrence_id == occurrence_id).then(|| state.clone()))
            .or_else(|| read_occurrence(transaction, occurrence_id).ok().flatten())
            .ok_or(StorageError::ScheduledAgentWorkflowNotFound)?;
        let attempt = begin_scheduled_agent_attempt(&occurrence, context.observed_at)
            .map_err(|_| StorageError::ScheduledAgentWorkflowConflict)?;
        attempts.push((occurrence, attempt));
    }
    Ok(ReconcilePlan {
        new_occurrences,
        attempts,
    })
}
