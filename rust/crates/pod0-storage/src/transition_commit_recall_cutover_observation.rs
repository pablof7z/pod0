use pod0_application::{
    DurableEffectExecution, DurableExternalEffectRequest, EffectOutcome,
    RecallIndexCutoverHostOutcome, RecallKnowledgeTransition, RecallObservationActivityInput,
    plan_recall_cutover_finalization, plan_recall_observation,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision, UnixTimestampMilliseconds};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::recall_cutover_store::{RecallIndexCutoverStage, StoredRecallIndexCutoverWorkflow};
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_recall_index_cutover_observation(
    path: &std::path::Path,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    observation: pod0_application::DurableRecallIndexCutoverHostObservation,
    committed_at: UnixTimestampMilliseconds,
) -> Result<(StoredRecallIndexCutoverWorkflow, bool), StorageError> {
    let request = request_for_intent(path, lease.intent_id)?;
    let fingerprint = observation_fingerprint(lease, &observation)?;
    let receipt = TransitionCommit::open(path)?.commit_planned_with_transaction_hooks(
        TransitionIngress {
            kind: TransitionIngressKind::HostObservation,
            id: observation.request_id.into_bytes(),
            fingerprint,
        },
        committed_at,
        |transaction| {
            let durable = validate(transaction, lease, &observation)?;
            let workflow = crate::recall_cutover_store::read(transaction)?
                .ok_or(StorageError::EvidenceNotFound)?;
            if workflow.command_id != durable.command_id
                || workflow.cancellation_id != durable.cancellation_id
                || workflow.stage != RecallIndexCutoverStage::AwaitingHost
            {
                return Err(StorageError::RevisionConflict);
            }
            let committed = next_core_revision(transaction)?;
            plan_recall_observation(RecallObservationActivityInput {
                command_id: durable.command_id,
                request_id: observation.request_id,
                current_revision: core_revision(transaction)?,
                committed_revision: committed,
                intent_id: lease.intent_id,
                attempt_id: lease.attempt_id,
                authorizing_activity_id: lease.authorizing_activity_id,
                correlation_id: lease.correlation_id,
                outcome: effect_outcome(&observation.outcome),
                transition: RecallKnowledgeTransition::IndexCutoverChanged,
                next_request: None,
            })
            .map(|plan| plan.map_mutation(|_| committed))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction| stage_attempt(transaction, lease, &observation, fingerprint),
        |transaction, _, committed| {
            let actual = crate::library_store::advance_playback_revision(transaction)?;
            if actual != committed { return Err(StorageError::RevisionConflict) }
            let (stage, removed) = crate::recall_cutover_store::observation_stage(&observation.outcome);
            transaction.execute(
                "UPDATE pod0_recall_index_cutover_workflow SET revision=?1,stage=?2,\
                 removed_file_count=?3,updated_at_ms=?4 WHERE singleton=1 AND stage='awaiting_host'",
                params![i64::try_from(committed.value).map_err(|_| StorageError::InvalidActivity)?,
                    stage, removed.map(i64::from), committed_at.value],
            ).map_err(|error| StorageError::sqlite("update recall cutover observation", error))?;
            Ok(committed)
        },
        |transaction| {
            crate::effect_outbox::complete_host_observation_in_transaction(transaction, lease)
                .map_err(|_| StorageError::RevisionConflict)
        },
    )?;
    let workflow = crate::LibraryStore::open_authoritative(path)?
        .recall_index_cutover_workflow()?
        .ok_or(StorageError::EvidenceNotFound)?;
    if workflow.command_id != request.command_id {
        return Err(StorageError::RevisionConflict);
    }
    Ok((workflow, receipt.replayed))
}

pub(crate) fn commit_recall_index_cutover_finalize(
    path: &std::path::Path,
    command_id: CommandId,
    removed_file_count: u32,
    committed_at: UnixTimestampMilliseconds,
) -> Result<StoredRecallIndexCutoverWorkflow, StorageError> {
    let internal_id = finalization_id(command_id);
    let fingerprint = ContentDigest::from_bytes(Sha256::digest(internal_id.into_bytes()).into());
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress { kind: TransitionIngressKind::Recovery, id: internal_id.into_bytes(), fingerprint },
        committed_at,
        |transaction| {
            let workflow = crate::recall_cutover_store::read(transaction)?
                .ok_or(StorageError::EvidenceNotFound)?;
            if workflow.command_id != command_id
                || workflow.stage
                    != (RecallIndexCutoverStage::HostObserved { removed_file_count })
            {
                return Err(StorageError::RevisionConflict);
            }
            let current = core_revision(transaction)?;
            let committed = next_core_revision(transaction)?;
            plan_recall_cutover_finalization(internal_id, current, committed)
                .map(|plan| plan.map_mutation(|_| committed))
                .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, _, committed| {
            let actual = crate::library_store::advance_playback_revision(transaction)?;
            if actual != committed { return Err(StorageError::RevisionConflict) }
            transaction.execute(
                "UPDATE pod0_recall_index_cutover_workflow SET revision=?1,stage='committed',\
                 updated_at_ms=?2 WHERE singleton=1 AND stage='host_observed' AND removed_file_count=?3",
                params![i64::try_from(committed.value).map_err(|_| StorageError::InvalidActivity)?,
                    committed_at.value, i64::from(removed_file_count)],
            ).map_err(|error| StorageError::sqlite("finalize recall cutover", error))?;
            Ok(committed)
        },
    )?;
    crate::LibraryStore::open_authoritative(path)?
        .recall_index_cutover_workflow()?
        .ok_or(StorageError::EvidenceNotFound)
}

fn request_for_intent(
    path: &std::path::Path,
    intent_id: pod0_domain::EffectIntentId,
) -> Result<pod0_application::DurableRecallIndexCutoverEffectRequest, StorageError> {
    crate::EffectOutbox::open(path)
        .map_err(|_| StorageError::InvalidActivity)?
        .effect_request(intent_id)
        .map_err(|_| StorageError::InvalidActivity)?
        .and_then(|request| match request.execution {
            DurableEffectExecution::RecallIndexCutover { request } => Some(request),
            _ => None,
        })
        .ok_or(StorageError::InvalidActivity)
}

fn validate(
    transaction: &rusqlite::Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    observation: &pod0_application::DurableRecallIndexCutoverHostObservation,
) -> Result<pod0_application::DurableRecallIndexCutoverEffectRequest, StorageError> {
    let payload: Option<String> = transaction.query_row(
        "SELECT i.request_json FROM pod0_effect_attempts a JOIN pod0_effect_intents i ON i.intent_id=a.intent_id WHERE a.lease_id=?1 AND a.attempt_id=?2 AND a.intent_id=?3 AND a.fence=?4 AND a.state_code=1 AND a.lease_expires_at_ms>=?5 AND a.lease_expires_at_ms=?6 AND i.authorizing_activity_id=?7 AND i.correlation_id=?8 AND i.effect_kind_code=3 AND i.subject_code=0",
        params![lease.lease_id.into_bytes().as_slice(), lease.attempt_id.into_bytes().as_slice(), lease.intent_id.into_bytes().as_slice(), i64::try_from(lease.fence).map_err(|_| StorageError::InvalidActivity)?, observation.observed_at.value, lease.expires_at.value, lease.authorizing_activity_id.into_bytes().as_slice(), lease.correlation_id.into_bytes().as_slice()], |row| row.get(0),
    ).optional().map_err(|error| StorageError::sqlite("validate recall cutover lease", error))?;
    let external: DurableExternalEffectRequest =
        serde_json::from_str(payload.as_deref().ok_or(StorageError::RevisionConflict)?)
            .map_err(|_| StorageError::InvalidActivity)?;
    let DurableEffectExecution::RecallIndexCutover { request } = external.execution else {
        return Err(StorageError::RevisionConflict);
    };
    if request.request_id != observation.request_id
        || request.cancellation_id != observation.cancellation_id
        || request.issued_revision != observation.observed_request_revision
    {
        return Err(StorageError::RevisionConflict);
    }
    Ok(request)
}

fn stage_attempt(
    transaction: &rusqlite::Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    observation: &pod0_application::DurableRecallIndexCutoverHostObservation,
    fingerprint: ContentDigest,
) -> Result<(), StorageError> {
    let changed = transaction.execute("UPDATE pod0_effect_attempts SET state_code=2,observation_schema_version=1,observation_json=?1,outcome_schema_version=1,outcome_json=?2,observed_at_ms=?3 WHERE lease_id=?4 AND fence=?5 AND state_code=1", params![hex(&fingerprint.into_bytes()), serde_json::to_string(&effect_outcome(&observation.outcome)).map_err(|_| StorageError::InvalidActivity)?, observation.observed_at.value, lease.lease_id.into_bytes().as_slice(), i64::try_from(lease.fence).map_err(|_| StorageError::InvalidActivity)?]).map_err(|error| StorageError::sqlite("stage recall cutover observation", error))?;
    (changed == 1)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}

fn effect_outcome(value: &RecallIndexCutoverHostOutcome) -> EffectOutcome {
    match value {
        RecallIndexCutoverHostOutcome::ArtifactsRemoved { .. } => EffectOutcome::Succeeded,
        RecallIndexCutoverHostOutcome::Cancelled => EffectOutcome::Cancelled,
        RecallIndexCutoverHostOutcome::Failed { code, .. } => {
            pod0_application::agent_host_failure_outcome(*code)
        }
    }
}
fn core_revision(c: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = c
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
fn next_core_revision(c: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    Ok(StateRevision::new(
        core_revision(c)?
            .value
            .checked_add(1)
            .ok_or(StorageError::InvalidActivity)?,
    ))
}
fn observation_fingerprint(
    lease: pod0_application::PersistedEffectLeaseIdentity,
    value: &pod0_application::DurableRecallIndexCutoverHostObservation,
) -> Result<ContentDigest, StorageError> {
    let mut hash = Sha256::new();
    hash.update(b"pod0/recall-cutover/observation/v1");
    hash.update(lease.attempt_id.into_bytes());
    hash.update(serde_json::to_vec(value).map_err(|_| StorageError::InvalidActivity)?);
    Ok(ContentDigest::from_bytes(hash.finalize().into()))
}
fn finalization_id(command_id: CommandId) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/recall-cutover/finalize/v1");
    hash.update(command_id.into_bytes());
    CommandId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
