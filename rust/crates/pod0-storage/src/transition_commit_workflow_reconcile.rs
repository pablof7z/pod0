use pod0_application::{
    ActivityDomain, ActivitySubject, DomainTransitionKind, DurableInternalCommandRequest,
    InternalCommandKind, InternalCommandOwnerActivityInput, LifecycleTransition,
    RequestDisposition, WorkflowOpportunity, WorkflowReconcileActivityInput,
    WorkflowReconcileIntent, WorkflowReconcilePlan, plan_internal_command_owner_activity,
    plan_workflow_reconcile_activity, plan_workflow_reconciliation_page,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision};

use super::TransitionCommit;
use crate::{
    StorageError, TransitionIngress, TransitionIngressKind, WorkflowReconcileCommitOutcome,
};

pub(crate) fn commit_workflow_reconcile(
    path: &std::path::Path,
    command_id: CommandId,
    fingerprint: ContentDigest,
    opportunity: WorkflowOpportunity,
    episode_offset: u32,
) -> Result<WorkflowReconcileCommitOutcome, StorageError> {
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: command_id.into_bytes(),
            fingerprint,
        },
        opportunity.observed_at,
        |transaction| {
            let current = current_revision(transaction)?;
            let configuration =
                crate::workflow_configuration_store::read_configuration(transaction)?;
            let capabilities =
                crate::workflow_configuration_store::read_capability_snapshot(transaction)?;
            let (disposition, plan) = match (configuration, capabilities) {
                (Some(configuration), Some(capabilities))
                    if capabilities.snapshot_id == opportunity.capability_snapshot_id =>
                {
                    let plan = plan_workflow_reconciliation_page(
                        &crate::listening_store_read::read_snapshot(transaction)?,
                        &configuration,
                        &capabilities,
                        episode_offset,
                    );
                    (RequestDisposition::Accepted, plan)
                }
                _ => (
                    RequestDisposition::Rejected {
                        reason: pod0_application::RequestRejectionReason::MissingPrerequisite,
                    },
                    WorkflowReconcilePlan {
                        intents: Vec::new(),
                        next_episode_offset: None,
                    },
                ),
            };
            plan_workflow_reconcile_activity(WorkflowReconcileActivityInput {
                command_id,
                current_revision: current,
                opportunity,
                plan,
                disposition,
            })
            .map(|plan| {
                plan.map_mutation(|mutation| {
                    (mutation, disposition == RequestDisposition::Accepted)
                })
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, current, (_mutation, accepted)| {
            if !accepted {
                return Ok(current);
            }
            let committed = crate::library_store::advance_playback_revision(transaction)?;
            (committed.value == current.value.saturating_add(1))
                .then_some(committed)
                .ok_or(StorageError::RevisionConflict)
        },
    )?;
    let fact_count = receipt
        .last_sequence
        .saturating_sub(receipt.first_sequence)
        .saturating_add(1);
    let authorized_command_count = fact_count.saturating_sub(2);
    Ok(WorkflowReconcileCommitOutcome {
        receipt,
        authorized_command_count: u16::try_from(authorized_command_count)
            .map_err(|_| StorageError::InvalidActivity)?,
    })
}

pub(crate) fn commit_workflow_reconcile_from_internal_command(
    path: &std::path::Path,
    command: crate::PendingInternalCommand,
) -> Result<WorkflowReconcileCommitOutcome, StorageError> {
    let InternalCommandKind::ContinueWorkflowReconciliation {
        opportunity,
        episode_offset,
    } = command.request.kind.clone()
    else {
        return Err(StorageError::InvalidActivity);
    };
    if command.request.target != ActivityDomain::Lifecycle
        || command.request.subject != ActivitySubject::Global
        || command.request.episode_id.is_some()
    {
        return Err(StorageError::InvalidActivity);
    }
    let command_id = CommandId::from_bytes(command.internal_command_id.into_bytes());
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::InternalCommand,
            id: command.internal_command_id.into_bytes(),
            fingerprint: internal_fingerprint(&command)?,
        },
        opportunity.observed_at,
        |transaction| {
            let current = current_revision(transaction)?;
            let configuration =
                crate::workflow_configuration_store::read_configuration(transaction)?;
            let capabilities =
                crate::workflow_configuration_store::read_capability_snapshot(transaction)?;
            let (disposition, plan) = match (configuration, capabilities) {
                (Some(configuration), Some(capabilities))
                    if capabilities.snapshot_id == opportunity.capability_snapshot_id =>
                {
                    (
                        RequestDisposition::Accepted,
                        plan_workflow_reconciliation_page(
                            &crate::listening_store_read::read_snapshot(transaction)?,
                            &configuration,
                            &capabilities,
                            episode_offset,
                        ),
                    )
                }
                _ => (
                    RequestDisposition::Rejected {
                        reason: pod0_application::RequestRejectionReason::MissingPrerequisite,
                    },
                    WorkflowReconcilePlan {
                        intents: Vec::new(),
                        next_episode_offset: None,
                    },
                ),
            };
            let accepted = disposition == RequestDisposition::Accepted;
            let committed = StateRevision::new(current.value + u64::from(accepted));
            let commands = if accepted {
                internal_requests(plan, opportunity)
            } else {
                Vec::new()
            };
            plan_internal_command_owner_activity(InternalCommandOwnerActivityInput {
                internal_command_id: command.internal_command_id,
                authorizing_activity_id: command.authorizing_activity_id,
                correlation_id: command.correlation_id,
                command_id,
                subject: ActivitySubject::Global,
                episode_id: None,
                current_revision: current,
                committed_revision: committed,
                disposition,
                transitions: accepted
                    .then_some((
                        ActivitySubject::Global,
                        DomainTransitionKind::Lifecycle(
                            LifecycleTransition::WorkflowReconciliationPlanned,
                        ),
                    ))
                    .into_iter()
                    .collect(),
                effects: Vec::new(),
                internal_commands: commands,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, current, mutation| {
            if !mutation.changes_state {
                return Ok(current);
            }
            let committed = crate::library_store::advance_playback_revision(transaction)?;
            (committed.value == current.value.saturating_add(1))
                .then_some(committed)
                .ok_or(StorageError::RevisionConflict)
        },
    )?;
    let fact_count = receipt
        .last_sequence
        .saturating_sub(receipt.first_sequence)
        .saturating_add(1);
    Ok(WorkflowReconcileCommitOutcome {
        receipt,
        authorized_command_count: u16::try_from(fact_count.saturating_sub(2))
            .map_err(|_| StorageError::InvalidActivity)?,
    })
}

fn internal_requests(
    plan: WorkflowReconcilePlan,
    opportunity: WorkflowOpportunity,
) -> Vec<DurableInternalCommandRequest> {
    let mut requests = plan
        .intents
        .into_iter()
        .map(|intent| match intent {
            WorkflowReconcileIntent::EnsurePublisherChapters { episode_id } => request(
                ActivityDomain::Chapter,
                Some(episode_id),
                InternalCommandKind::EnsurePublisherChapters,
            ),
            WorkflowReconcileIntent::EnsureTranscript {
                episode_id,
                origin,
                configuration,
            } => request(
                ActivityDomain::Transcript,
                Some(episode_id),
                InternalCommandKind::EnsureTranscriptWorkflow {
                    origin,
                    configuration,
                },
            ),
            WorkflowReconcileIntent::EnsureModelChapters {
                episode_id,
                configured_model,
            } => request(
                ActivityDomain::Chapter,
                Some(episode_id),
                InternalCommandKind::EnsureModelChapters { configured_model },
            ),
            WorkflowReconcileIntent::ReconcileScheduledRuns => request(
                ActivityDomain::ScheduledAgent,
                None,
                InternalCommandKind::ReconcileScheduledRuns,
            ),
        })
        .collect::<Vec<_>>();
    if let Some(episode_offset) = plan.next_episode_offset {
        requests.push(request(
            ActivityDomain::Lifecycle,
            None,
            InternalCommandKind::ContinueWorkflowReconciliation {
                opportunity,
                episode_offset,
            },
        ));
    }
    requests
}

fn request(
    target: ActivityDomain,
    episode_id: Option<pod0_domain::EpisodeId>,
    kind: InternalCommandKind,
) -> DurableInternalCommandRequest {
    DurableInternalCommandRequest {
        kind,
        target,
        subject: episode_id.map_or(ActivitySubject::Global, |episode_id| {
            ActivitySubject::Episode { episode_id }
        }),
        episode_id,
    }
}

fn internal_fingerprint(
    command: &crate::PendingInternalCommand,
) -> Result<ContentDigest, StorageError> {
    use sha2::{Digest as _, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"pod0/workflow-reconcile/internal/v1");
    hash.update(command.internal_command_id.into_bytes());
    hash.update(serde_json::to_vec(&command.request).map_err(|_| StorageError::InvalidActivity)?);
    Ok(ContentDigest::from_bytes(hash.finalize().into()))
}

fn current_revision(connection: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read workflow reconcile revision", error))?;
    Ok(StateRevision::new(
        u64::try_from(value).map_err(|_| StorageError::InvalidActivity)?,
    ))
}
