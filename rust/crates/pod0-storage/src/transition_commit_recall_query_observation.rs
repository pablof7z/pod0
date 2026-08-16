use pod0_application::{
    AgentRecallEffectPhase, AgentRecallHostOutcome, DurableEffectExecution,
    DurableExternalEffectRequest, EffectOutcome, RecallObservationActivityInput,
    RecallQueryResolution, StoredRecallQueryWorkflow, plan_recall_observation,
};
use pod0_domain::{ContentDigest, StateRevision, UnixTimestampMilliseconds};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_recall_query_observation(
    path: &std::path::Path,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    observation: pod0_application::DurableAgentRecallHostObservation,
    resolution: RecallQueryResolution,
    committed_at: UnixTimestampMilliseconds,
) -> Result<(StoredRecallQueryWorkflow, bool), StorageError> {
    let request = crate::EffectOutbox::open(path)
        .map_err(|_| StorageError::InvalidActivity)?
        .effect_request(lease.intent_id)
        .map_err(|_| StorageError::InvalidActivity)?
        .and_then(|value| match value.execution {
            DurableEffectExecution::RecallQuery { request } => Some(request),
            _ => None,
        })
        .ok_or(StorageError::InvalidActivity)?;
    let fingerprint = fingerprint(lease, &observation, &resolution)?;
    let receipt = TransitionCommit::open(path)?.commit_planned_with_transaction_hooks(
        TransitionIngress {
            kind: TransitionIngressKind::HostObservation,
            id: observation.request_id.into_bytes(),
            fingerprint,
        },
        committed_at,
        |transaction| {
            let request = validate(transaction, lease, &observation)?;
            let workflow =
                crate::recall_workflow_store::read_query(transaction, request.query.query_id)?
                    .ok_or(StorageError::EvidenceNotFound)?;
            let committed = next_core_revision(transaction)?;
            let next = match &resolution {
                RecallQueryResolution::Rerank { request } => Some(request.clone()),
                _ => None,
            };
            validate_resolution(&request, &observation.outcome, &resolution)?;
            plan_recall_observation(RecallObservationActivityInput {
                command_id: workflow.command_id,
                request_id: observation.request_id,
                current_revision: core_revision(transaction)?,
                committed_revision: committed,
                intent_id: lease.intent_id,
                attempt_id: lease.attempt_id,
                authorizing_activity_id: lease.authorizing_activity_id,
                correlation_id: lease.correlation_id,
                outcome: outcome(&observation.outcome),
                transition: pod0_application::RecallKnowledgeTransition::QueryStateChanged,
                next_request: next,
            })
            .map(|plan| plan.map_mutation(|_| (workflow, committed)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction| stage(transaction, lease, &observation, fingerprint),
        |transaction, _, (mut workflow, committed)| {
            let actual = crate::library_store::advance_playback_revision(transaction)?;
            if actual != committed {
                return Err(StorageError::RevisionConflict);
            }
            workflow.revision = committed;
            workflow.updated_at = committed_at;
            match &resolution {
                RecallQueryResolution::Rerank { .. } => {
                    workflow.stage = pod0_application::RecallStage::Running {
                        phase: pod0_application::RecallPhase::Reranking,
                    }
                }
                RecallQueryResolution::Finish {
                    stage,
                    evidence,
                    failure,
                } => {
                    workflow.stage = *stage;
                    workflow.evidence = evidence.clone();
                    workflow.failure = failure.clone();
                }
            }
            crate::recall_workflow_store::update_query(transaction, &workflow)?;
            Ok(committed)
        },
        |transaction| {
            crate::effect_outbox::complete_host_observation_in_transaction(transaction, lease)
                .map_err(|_| StorageError::RevisionConflict)
        },
    )?;
    let workflow = crate::LibraryStore::open_authoritative(path)?
        .recall_query_workflow(request.query.query_id)?
        .ok_or(StorageError::EvidenceNotFound)?;
    Ok((workflow, receipt.replayed))
}

fn validate(
    transaction: &rusqlite::Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    observation: &pod0_application::DurableAgentRecallHostObservation,
) -> Result<pod0_application::DurableRecallQueryEffectRequest, StorageError> {
    let payload: Option<String> = transaction.query_row(
        "SELECT i.request_json FROM pod0_effect_attempts a JOIN pod0_effect_intents i ON i.intent_id=a.intent_id WHERE a.lease_id=?1 AND a.attempt_id=?2 AND a.intent_id=?3 AND a.fence=?4 AND a.state_code=1 AND a.lease_expires_at_ms>=?5 AND a.lease_expires_at_ms=?6 AND i.authorizing_activity_id=?7 AND i.correlation_id=?8 AND i.effect_kind_code=3 AND i.subject_code=0",
        params![lease.lease_id.into_bytes().as_slice(), lease.attempt_id.into_bytes().as_slice(), lease.intent_id.into_bytes().as_slice(), i64::try_from(lease.fence).map_err(|_| StorageError::InvalidActivity)?, observation.observed_at.value, lease.expires_at.value, lease.authorizing_activity_id.into_bytes().as_slice(), lease.correlation_id.into_bytes().as_slice()],
        |row| row.get(0),
    ).optional().map_err(|error| StorageError::sqlite("validate recall query lease", error))?;
    let request: DurableExternalEffectRequest =
        serde_json::from_str(payload.as_deref().ok_or(StorageError::RevisionConflict)?)
            .map_err(|_| StorageError::InvalidActivity)?;
    let DurableEffectExecution::RecallQuery { request } = request.execution else {
        return Err(StorageError::RevisionConflict);
    };
    if request.request_id != observation.request_id
        || request.cancellation_id != observation.cancellation_id
        || request.issued_revision != observation.observed_request_revision
        || !outcome_matches(&request, &observation.outcome)
    {
        return Err(StorageError::RevisionConflict);
    }
    Ok(request)
}

fn outcome_matches(
    request: &pod0_application::DurableRecallQueryEffectRequest,
    outcome: &AgentRecallHostOutcome,
) -> bool {
    match (outcome, &request.phase) {
        (
            AgentRecallHostOutcome::QueryEmbedded { query_id, .. },
            AgentRecallEffectPhase::EmbedQuery,
        )
        | (
            AgentRecallHostOutcome::CandidatesReranked { query_id, .. },
            AgentRecallEffectPhase::Rerank { .. },
        ) => *query_id == request.query.query_id,
        (AgentRecallHostOutcome::Failed { .. }, _) | (AgentRecallHostOutcome::Cancelled, _) => true,
        _ => false,
    }
}

fn validate_resolution(
    request: &pod0_application::DurableRecallQueryEffectRequest,
    outcome: &AgentRecallHostOutcome,
    resolution: &RecallQueryResolution,
) -> Result<(), StorageError> {
    let phase_matches = matches!(
        (&request.phase, outcome),
        (
            AgentRecallEffectPhase::EmbedQuery,
            AgentRecallHostOutcome::QueryEmbedded { .. }
        ) | (
            AgentRecallEffectPhase::Rerank { .. },
            AgentRecallHostOutcome::CandidatesReranked { .. }
        ) | (_, AgentRecallHostOutcome::Failed { .. })
            | (_, AgentRecallHostOutcome::Cancelled)
    );
    if !phase_matches {
        return Err(StorageError::RevisionConflict);
    }
    if let RecallQueryResolution::Rerank { request: next } = resolution {
        if !matches!(request.phase, AgentRecallEffectPhase::EmbedQuery)
            || !matches!(next.phase, AgentRecallEffectPhase::Rerank { .. })
            || request.command_id != next.command_id
            || request.cancellation_id != next.cancellation_id
            || request.query != next.query
            || request.issued_revision != next.issued_revision
            || request.embedding_provider != next.embedding_provider
            || request.embedding_model != next.embedding_model
            || request.reranker != next.reranker
            || next.request_id
                != pod0_application::recall_query_request_id(next.query.query_id, &next.phase)
        {
            return Err(StorageError::RevisionConflict);
        }
        let AgentRecallEffectPhase::Rerank {
            candidates,
            evidence,
        } = &next.phase
        else {
            return Err(StorageError::RevisionConflict);
        };
        if candidates.is_empty()
            || candidates.len() != evidence.len()
            || !candidates.iter().zip(evidence).all(|(candidate, item)| {
                candidate.span_id == item.span_id && candidate.excerpt == item.excerpt
            })
        {
            return Err(StorageError::RevisionConflict);
        }
    }
    Ok(())
}

fn stage(
    transaction: &rusqlite::Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    observation: &pod0_application::DurableAgentRecallHostObservation,
    fingerprint: ContentDigest,
) -> Result<(), StorageError> {
    let changed = transaction.execute("UPDATE pod0_effect_attempts SET state_code=2,observation_schema_version=1,observation_json=?1,outcome_schema_version=1,outcome_json=?2,observed_at_ms=?3 WHERE lease_id=?4 AND fence=?5 AND state_code=1", params![hex(&fingerprint.into_bytes()), serde_json::to_string(&outcome(&observation.outcome)).map_err(|_| StorageError::InvalidActivity)?, observation.observed_at.value, lease.lease_id.into_bytes().as_slice(), i64::try_from(lease.fence).map_err(|_| StorageError::InvalidActivity)?]).map_err(|error| StorageError::sqlite("stage recall query observation", error))?;
    (changed == 1)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}

fn outcome(value: &AgentRecallHostOutcome) -> EffectOutcome {
    match value {
        AgentRecallHostOutcome::QueryEmbedded { .. }
        | AgentRecallHostOutcome::CandidatesReranked { .. } => EffectOutcome::Succeeded,
        AgentRecallHostOutcome::Cancelled => EffectOutcome::Cancelled,
        AgentRecallHostOutcome::Failed { code, .. } => {
            pod0_application::agent_host_failure_outcome(*code)
        }
    }
}
fn core_revision(c: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let v: i64 = c
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .map_err(|e| StorageError::sqlite("read recall core revision", e))?;
    Ok(StateRevision::new(
        u64::try_from(v).map_err(|_| StorageError::InvalidActivity)?,
    ))
}
fn next_core_revision(c: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let v = core_revision(c)?
        .value
        .checked_add(1)
        .ok_or(StorageError::InvalidActivity)?;
    Ok(StateRevision::new(v))
}
fn fingerprint(
    lease: pod0_application::PersistedEffectLeaseIdentity,
    o: &pod0_application::DurableAgentRecallHostObservation,
    r: &RecallQueryResolution,
) -> Result<ContentDigest, StorageError> {
    let mut h = Sha256::new();
    h.update(b"pod0/recall-query/observation/v1");
    h.update(lease.attempt_id.into_bytes());
    h.update(serde_json::to_vec(o).map_err(|_| StorageError::InvalidActivity)?);
    h.update(serde_json::to_vec(r).map_err(|_| StorageError::InvalidActivity)?);
    Ok(ContentDigest::from_bytes(h.finalize().into()))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
