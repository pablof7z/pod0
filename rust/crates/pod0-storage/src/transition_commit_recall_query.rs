use pod0_application::{
    AgentRecallEffectPhase, DurableRecallQueryEffectRequest, RecallStage,
    RecallWorkflowActivityInput, RecallWorkflowEffect, RequestDisposition,
    StoredRecallQueryWorkflow, plan_recall_workflow_activity, recall_query_request_id,
};
use pod0_domain::{CancellationId, CommandId, StateRevision, UnixTimestampMilliseconds};

use super::TransitionCommit;
use super::application_support::{fingerprint, legacy_library_receipt, next_core_revision};
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_recall_query_start(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    cancellation_id: CancellationId,
    mut query: pod0_application::RecallQuery,
    initial_stage: RecallStage,
    initial_failure: Option<pod0_application::CoreFailure>,
    observed_at: UnixTimestampMilliseconds,
) -> Result<StoredRecallQueryWorkflow, StorageError> {
    query.text = query.text.split_whitespace().collect::<Vec<_>>().join(" ");
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
            let committed = next_core_revision(transaction, "read recall query core revision")?;
            let duplicate = legacy_library_receipt(
                transaction,
                command_id,
                command_fingerprint,
                "read recall query receipt",
            )?;
            let existing = crate::recall_workflow_store::read_query(transaction, query.query_id)?;
            let configuration = crate::recall_configuration_store::read_configuration(transaction)?
                .unwrap_or_default();
            let disposition = if duplicate.is_some() {
                RequestDisposition::Duplicate
            } else if existing.is_some() {
                return Err(StorageError::CommandConflict);
            } else {
                RequestDisposition::Accepted
            };
            let phase = AgentRecallEffectPhase::EmbedQuery;
            let deadline_at =
                UnixTimestampMilliseconds::new(observed_at.value.saturating_add(30_000));
            let request = (disposition == RequestDisposition::Accepted
                && !initial_stage.is_terminal())
            .then(|| DurableRecallQueryEffectRequest {
                command_id,
                cancellation_id,
                request_id: recall_query_request_id(query.query_id, &phase),
                issued_revision: committed,
                deadline_at,
                query: query.clone(),
                embedding_provider: configuration.embedding_provider,
                embedding_model: configuration.embedding_model,
                reranker: configuration
                    .reranker_provider
                    .zip(configuration.reranker_model),
                phase,
            });
            plan_recall_workflow_activity(RecallWorkflowActivityInput {
                command_id,
                current_revision: current,
                committed_revision: if disposition == RequestDisposition::Accepted {
                    committed
                } else {
                    current
                },
                disposition,
                transition: pod0_application::RecallKnowledgeTransition::QueryStateChanged,
                effect: request.map(RecallWorkflowEffect::Query),
            })
            .map(|plan| plan.map_mutation(|_| (existing, committed)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, (existing, committed)| {
            let was_existing = existing.is_some();
            let workflow = if let Some(existing) = existing {
                existing
            } else {
                let workflow = StoredRecallQueryWorkflow {
                    command_id,
                    cancellation_id,
                    query: query.clone(),
                    revision: committed,
                    stage: initial_stage,
                    evidence: Vec::new(),
                    failure: initial_failure.clone(),
                    created_at: observed_at,
                    updated_at: observed_at,
                };
                crate::recall_workflow_store::insert_query(transaction, &workflow)?;
                let actual = crate::library_store::finish_command(
                    transaction,
                    command_id,
                    command_fingerprint,
                    observed_at.value,
                )?;
                if actual != committed {
                    return Err(StorageError::RevisionConflict);
                }
                workflow
            };
            *output.borrow_mut() = Some(workflow);
            Ok(if was_existing { expected } else { committed })
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
        .map_err(|error| StorageError::sqlite("read recall query core revision", error))?;
    Ok(StateRevision::new(
        u64::try_from(value).map_err(|_| StorageError::InvalidActivity)?,
    ))
}
