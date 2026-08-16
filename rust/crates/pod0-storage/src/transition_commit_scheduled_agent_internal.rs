use std::cell::RefCell;

use pod0_application::{
    ActivityDomain, ActivitySubject, DomainTransitionKind, DurableEffectExecution,
    DurableExternalEffectRequest, ExternalEffectKind, InternalCommandKind,
    InternalCommandOwnerActivityInput, RequestDisposition, ScheduledAgentActivityTransition,
    plan_internal_command_owner_activity,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::scheduled_agent_store_read::pending_requests;
use crate::scheduled_agent_store_reconcile::{insert_occurrence, persist_attempt};
use crate::scheduled_agent_store_tasks::{advance_core_revision, core_revision};
use crate::{
    PendingInternalCommand, ScheduledAgentCommandContext, ScheduledAgentReconcileOutcome,
    StorageError, TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_scheduled_agent_internal_reconcile(
    path: &std::path::Path,
    command: PendingInternalCommand,
    mut context: ScheduledAgentCommandContext,
) -> Result<ScheduledAgentReconcileOutcome, StorageError> {
    validate(&command)?;
    context.command_id = CommandId::from_bytes(command.internal_command_id.into_bytes());
    let planned = RefCell::new(None::<super::scheduled_agent_reconcile::ReconcilePlan>);
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::InternalCommand,
            id: command.internal_command_id.into_bytes(),
            fingerprint: fingerprint(&command),
        },
        context.observed_at,
        |transaction| {
            let current = core_revision(transaction)?;
            let plan = super::scheduled_agent_reconcile::build_plan(transaction, &context)?;
            let mut transitions = plan
                .new_occurrences
                .iter()
                .map(|(_, state)| {
                    (
                        ActivitySubject::ScheduledOccurrence {
                            occurrence_id: state.occurrence_id,
                        },
                        DomainTransitionKind::ScheduledAgent(
                            ScheduledAgentActivityTransition::OccurrenceStateChanged,
                        ),
                    )
                })
                .collect::<Vec<_>>();
            transitions.extend(plan.attempts.iter().map(|(_, attempt)| {
                (
                    ActivitySubject::ScheduledOccurrence {
                        occurrence_id: attempt.state.occurrence_id,
                    },
                    DomainTransitionKind::ScheduledAgent(
                        ScheduledAgentActivityTransition::AttemptStateChanged,
                    ),
                )
            }));
            let effects = plan
                .attempts
                .iter()
                .map(|(_, attempt)| DurableExternalEffectRequest {
                    kind: ExternalEffectKind::ScheduledAgentProvider,
                    subject: ActivitySubject::ScheduledOccurrence {
                        occurrence_id: attempt.state.occurrence_id,
                    },
                    episode_id: None,
                    not_before: None,
                    deadline_at: Some(attempt.deadline_at),
                    execution: DurableEffectExecution::ScheduledAgent {
                        request: pod0_application::DurableScheduledAgentEffectRequest {
                            request_id: attempt.request_id,
                            command_id: context.command_id,
                            cancellation_id: context.cancellation_id,
                            issued_revision: context.issued_revision,
                            deadline_at: attempt.deadline_at,
                            execution: attempt.request.clone(),
                        },
                    },
                })
                .collect();
            *planned.borrow_mut() = Some(plan);
            plan_internal_command_owner_activity(InternalCommandOwnerActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                command_id: context.command_id,
                subject: ActivitySubject::Global,
                episode_id: None,
                current_revision: current,
                committed_revision: StateRevision::new(
                    current
                        .value
                        .checked_add(1)
                        .ok_or(StorageError::InvalidActivity)?,
                ),
                disposition: RequestDisposition::Accepted,
                transitions,
                effects,
                internal_commands: Vec::new(),
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, _| {
            let plan = planned.borrow();
            let plan = plan.as_ref().ok_or(StorageError::InvalidActivity)?;
            for (definition, occurrence) in &plan.new_occurrences {
                insert_occurrence(transaction, definition, occurrence)?;
            }
            for (previous, attempt) in &plan.attempts {
                persist_attempt(transaction, &context, previous, attempt)?;
            }
            let revision = advance_core_revision(transaction)?;
            (revision.value == expected.value.saturating_add(1))
                .then_some(revision)
                .ok_or(StorageError::RevisionConflict)
        },
    )?;
    let store = crate::ScheduledAgentStore::open_authoritative(path)?;
    Ok(ScheduledAgentReconcileOutcome {
        created_occurrences: if receipt.replayed {
            Vec::new()
        } else {
            planned
                .borrow()
                .as_ref()
                .map(|plan| {
                    plan.new_occurrences
                        .iter()
                        .map(|(_, state)| state.occurrence_id)
                        .collect()
                })
                .unwrap_or_default()
        },
        requests: store.read(|connection| {
            pending_requests(
                connection,
                (!receipt.replayed).then_some(context.command_id),
                u16::MAX,
            )
        })?,
    })
}

fn validate(command: &PendingInternalCommand) -> Result<(), StorageError> {
    if command.request.target != ActivityDomain::ScheduledAgent
        || command.request.episode_id.is_some()
        || !matches!(
            command.request.kind,
            InternalCommandKind::ReconcileScheduledRuns
        )
    {
        return Err(StorageError::InvalidActivity);
    }
    Ok(())
}

fn fingerprint(command: &PendingInternalCommand) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/scheduled-agent/internal-reconcile/v1");
    hash.update(command.internal_command_id.into_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}
