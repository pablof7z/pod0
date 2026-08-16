use pod0_application::{
    DurableRecallIndexCutoverEffectRequest, RecallKnowledgeTransition, RecallWorkflowActivityInput,
    RecallWorkflowEffect, RequestDisposition, RequestRejectionReason,
    plan_recall_workflow_activity,
};
use pod0_domain::{CancellationId, CommandId, StateRevision, UnixTimestampMilliseconds};

use super::TransitionCommit;
use super::application_support::{fingerprint, next_core_revision};
use crate::recall_cutover_store::{
    RecallIndexCutoverStage, RecallIndexCutoverStartOutcome, StoredRecallIndexCutoverWorkflow,
};
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_recall_index_cutover_start(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    cancellation_id: CancellationId,
    already_committed: bool,
    prerequisites_ready: bool,
    observed_at: UnixTimestampMilliseconds,
) -> Result<RecallIndexCutoverStartOutcome, StorageError> {
    let ingress = TransitionIngress {
        kind: TransitionIngressKind::ApplicationCommand,
        id: command_id.into_bytes(),
        fingerprint: fingerprint(command_fingerprint)?,
    };
    let output = std::cell::RefCell::new(None);
    TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        observed_at,
        |transaction| {
            let current = core_revision(transaction)?;
            let existing = crate::recall_cutover_store::read(transaction)?;
            let (disposition, outcome) = if already_committed
                || existing.is_some_and(|workflow| {
                    matches!(workflow.stage, RecallIndexCutoverStage::Committed { .. })
                }) {
                (
                    RequestDisposition::AlreadyComplete,
                    RecallIndexCutoverStartOutcome::AlreadyComplete,
                )
            } else if !prerequisites_ready {
                (
                    RequestDisposition::Rejected {
                        reason: RequestRejectionReason::MissingPrerequisite,
                    },
                    RecallIndexCutoverStartOutcome::MissingPrerequisite,
                )
            } else if existing.is_some() {
                return Err(StorageError::CommandConflict);
            } else {
                let committed =
                    next_core_revision(transaction, "read recall cutover core revision")?;
                let workflow = StoredRecallIndexCutoverWorkflow {
                    command_id,
                    cancellation_id,
                    revision: committed,
                    stage: RecallIndexCutoverStage::AwaitingHost,
                };
                (
                    RequestDisposition::Accepted,
                    RecallIndexCutoverStartOutcome::Authorized(workflow),
                )
            };
            let committed = match outcome {
                RecallIndexCutoverStartOutcome::Authorized(workflow) => workflow.revision,
                _ => current,
            };
            let request =
                matches!(outcome, RecallIndexCutoverStartOutcome::Authorized(_)).then(|| {
                    let deadline_at =
                        UnixTimestampMilliseconds::new(observed_at.value.saturating_add(60_000));
                    DurableRecallIndexCutoverEffectRequest {
                        command_id,
                        cancellation_id,
                        request_id: pod0_domain::HostRequestId::from_bytes(command_id.into_bytes()),
                        issued_revision: committed,
                        deadline_at,
                    }
                });
            plan_recall_workflow_activity(RecallWorkflowActivityInput {
                command_id,
                current_revision: current,
                committed_revision: committed,
                disposition,
                transition: RecallKnowledgeTransition::IndexCutoverChanged,
                effect: request.map(RecallWorkflowEffect::Cutover),
            })
            .map(|plan| plan.map_mutation(|_| outcome))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, outcome| {
            if let RecallIndexCutoverStartOutcome::Authorized(workflow) = outcome {
                transaction
                    .execute(
                        "INSERT INTO pod0_recall_index_cutover_workflow(singleton,command_id,\
                     cancellation_id,revision,stage,removed_file_count,updated_at_ms) \
                     VALUES(1,?1,?2,?3,'awaiting_host',NULL,?4)",
                        rusqlite::params![
                            workflow.command_id.into_bytes().as_slice(),
                            workflow.cancellation_id.into_bytes().as_slice(),
                            i64::try_from(workflow.revision.value)
                                .map_err(|_| StorageError::InvalidActivity)?,
                            observed_at.value,
                        ],
                    )
                    .map_err(|error| {
                        StorageError::sqlite("insert recall cutover workflow", error)
                    })?;
                let actual = crate::library_store::finish_command(
                    transaction,
                    command_id,
                    command_fingerprint,
                    observed_at.value,
                )?;
                if actual != workflow.revision {
                    return Err(StorageError::RevisionConflict);
                }
                *output.borrow_mut() = Some(outcome);
                Ok(workflow.revision)
            } else {
                *output.borrow_mut() = Some(outcome);
                Ok(expected)
            }
        },
    )?;
    output.into_inner().ok_or(StorageError::InvalidActivity)
}

fn core_revision(connection: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read recall cutover core revision", error))?;
    Ok(StateRevision::new(
        u64::try_from(value).map_err(|_| StorageError::InvalidActivity)?,
    ))
}
