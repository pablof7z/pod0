use pod0_application::{
    EffectOutcome, LibraryNetworkMutation, LibraryNetworkStep, plan_library_network_observation,
};
use pod0_domain::{StateRevision, UnixTimestampMilliseconds};
use rusqlite::{OptionalExtension, params};

use crate::{
    LibraryNetworkObservationAction, LibraryNetworkObservationInput, LibraryNetworkWorkflowRecord,
    StorageError, StoredLibraryNetworkStage, TransitionIngress, TransitionIngressKind,
    effect_outbox::complete_host_observation_in_transaction,
    library_network_store::{read_workflow, serialize, update_result},
    transition_commit::TransitionCommit,
    transition_commit_library_network_admission::request_id,
    transition_commit_library_network_observation_support::{
        current_revision, next_effect, next_revision, observation_fingerprint,
        observation_identity, require_revision, set_revision, shared_episode_id,
    },
};

impl crate::LibraryStore {
    pub fn commit_library_network_observation(
        &self,
        input: LibraryNetworkObservationInput,
    ) -> Result<LibraryNetworkWorkflowRecord, StorageError> {
        let identity = observation_identity(input.lease.attempt_id, input.sequence_number);
        let fingerprint = observation_fingerprint(&input)?;
        let planned = input.action.clone();
        let applied = input.action.clone();
        TransitionCommit::open(self.path())?.commit_planned_with_transaction_hooks(
            TransitionIngress {
                kind: TransitionIngressKind::HostObservation,
                id: identity.into_bytes(),
                fingerprint,
            },
            UnixTimestampMilliseconds::new(input.observed_at_ms.max(0)),
            |transaction| {
                let current = read_workflow(transaction, input.command_id)?
                    .ok_or(StorageError::EntityNotFound)?;
                validate_pending(&current, &input)?;
                let revision = current_revision(transaction)?;
                let next = next_effect(
                    &current,
                    &planned,
                    next_revision(revision)?,
                    input.observed_at_ms,
                )?;
                plan_library_network_observation(
                    identity,
                    input.lease.intent_id,
                    input.lease.authorizing_activity_id,
                    input.lease.correlation_id,
                    input.command_id,
                    input.request_id,
                    shared_episode_id(transaction, &planned)?,
                    revision,
                    outcome(&planned),
                    next,
                )
                .map_err(|_| StorageError::InvalidActivity)
            },
            |transaction| stage_observation(transaction, &input),
            |transaction, expected, mutation| {
                if mutation != LibraryNetworkMutation::Apply {
                    return Err(StorageError::InvalidActivity);
                }
                require_revision(transaction, expected)?;
                let current = read_workflow(transaction, input.command_id)?
                    .ok_or(StorageError::EntityNotFound)?;
                validate_pending(&current, &input)?;
                let committed = next_revision(expected)?;
                apply_action(
                    transaction,
                    &current,
                    applied,
                    committed,
                    input.observed_at_ms,
                )?;
                set_revision(transaction, committed)?;
                Ok(committed)
            },
            |transaction| {
                complete_host_observation_in_transaction(transaction, input.lease)
                    .map_err(|_| StorageError::InvalidActivity)
            },
        )?;
        self.library_network_workflow(input.command_id)?
            .ok_or(StorageError::EntityNotFound)
    }
}

fn apply_action(
    transaction: &rusqlite::Transaction<'_>,
    current: &LibraryNetworkWorkflowRecord,
    action: LibraryNetworkObservationAction,
    revision: StateRevision,
    now_ms: i64,
) -> Result<(), StorageError> {
    match action {
        LibraryNetworkObservationAction::CompleteDirectory { results }
        | LibraryNetworkObservationAction::CompleteTopLookup { results } => {
            let result = crate::StoredLibraryNetworkResult::Directory { entries: results };
            update_result(
                transaction,
                current.command_id,
                StoredLibraryNetworkStage::Completed,
                revision,
                Some(&result),
                None,
                now_ms,
            )
        }
        LibraryNetworkObservationAction::ContinueTopLookup { ranked_ids, .. } => {
            let step = LibraryNetworkStep::DirectoryLookup { ranked_ids };
            continue_workflow(transaction, current.command_id, revision, &step, now_ms)
        }
        LibraryNetworkObservationAction::ContinueShared { step, .. } => {
            continue_workflow(transaction, current.command_id, revision, &step, now_ms)
        }
        LibraryNetworkObservationAction::ContinueCatalog { step, .. } => {
            continue_workflow(transaction, current.command_id, revision, &step, now_ms)
        }
        LibraryNetworkObservationAction::CompleteShared { episode } => {
            let episode_id = crate::library_store_external_apply::apply_resolved_shared_episode(
                transaction,
                &episode,
                now_ms,
            )?;
            let result = crate::StoredLibraryNetworkResult::SharedEpisode { episode_id };
            update_result(
                transaction,
                current.command_id,
                StoredLibraryNetworkStage::Completed,
                revision,
                Some(&result),
                None,
                now_ms,
            )
        }
        LibraryNetworkObservationAction::CompleteCatalog { candidates } => {
            let result = crate::library_store_external_apply::apply_catalog_results(
                transaction,
                candidates,
                now_ms,
            )?;
            update_result(
                transaction,
                current.command_id,
                StoredLibraryNetworkStage::Completed,
                revision,
                Some(&result),
                None,
                now_ms,
            )
        }
        LibraryNetworkObservationAction::Fail { code } => update_result(
            transaction,
            current.command_id,
            StoredLibraryNetworkStage::Failed,
            revision,
            None,
            Some(&code),
            now_ms,
        ),
        LibraryNetworkObservationAction::Cancel => update_result(
            transaction,
            current.command_id,
            StoredLibraryNetworkStage::Cancelled,
            revision,
            None,
            None,
            now_ms,
        ),
    }
}

fn continue_workflow(
    transaction: &rusqlite::Transaction<'_>,
    command_id: pod0_domain::CommandId,
    revision: StateRevision,
    step: &LibraryNetworkStep,
    now_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_library_network_workflows SET stage='awaiting_followup',revision=?1,\
         pending_request_id=?2,pending_step_json=?3,updated_at_ms=?4 WHERE command_id=?5",
            params![
                i64::try_from(revision.value).map_err(|_| StorageError::InvalidActivity)?,
                request_id(command_id, step).into_bytes().as_slice(),
                serialize(step)?,
                now_ms,
                command_id.into_bytes().as_slice()
            ],
        )
        .map_err(|error| StorageError::sqlite("continue library network workflow", error))?;
    Ok(())
}

fn validate_pending(
    current: &LibraryNetworkWorkflowRecord,
    input: &LibraryNetworkObservationInput,
) -> Result<(), StorageError> {
    if current.stage.is_terminal()
        || current.pending_request_id != Some(input.request_id)
        || current.cancellation_id != input.cancellation_id
        || current.revision != input.observed_request_revision
    {
        return Err(StorageError::RevisionConflict);
    }
    Ok(())
}

fn stage_observation(
    transaction: &rusqlite::Transaction<'_>,
    input: &LibraryNetworkObservationInput,
) -> Result<(), StorageError> {
    let fence = i64::try_from(input.lease.fence).map_err(|_| StorageError::InvalidActivity)?;
    let observation_json = serialize(&input.observation)?;
    let outcome_json = serialize(&outcome(&input.action))?;
    let state: Option<i64> = transaction.query_row(
        "SELECT a.state_code FROM pod0_effect_attempts a JOIN pod0_effect_intents i ON i.intent_id=a.intent_id \
         JOIN pod0_library_network_workflows w ON w.command_id=i.subject_id \
         WHERE a.lease_id=?1 AND a.fence=?2 AND a.attempt_id=?3 AND a.intent_id=?4 \
         AND i.authorizing_activity_id=?5 AND i.correlation_id=?6 AND a.lease_expires_at_ms>=?7 \
         AND a.lease_expires_at_ms=?8 AND i.effect_kind_code=17 AND w.pending_request_id=?9 \
         AND w.cancellation_id=?10 AND w.revision=?11",
        params![input.lease.lease_id.into_bytes().as_slice(), fence,
            input.lease.attempt_id.into_bytes().as_slice(), input.lease.intent_id.into_bytes().as_slice(),
            input.lease.authorizing_activity_id.into_bytes().as_slice(), input.lease.correlation_id.into_bytes().as_slice(),
            input.observed_at_ms, input.lease.expires_at.value, input.request_id.into_bytes().as_slice(),
            input.cancellation_id.into_bytes().as_slice(), i64::try_from(input.observed_request_revision.value).map_err(|_| StorageError::InvalidActivity)?],
        |row| row.get(0),
    ).optional().map_err(|error| StorageError::sqlite("validate library network lease", error))?;
    if state != Some(1) {
        return Err(StorageError::RevisionConflict);
    }
    transaction.execute(
        "UPDATE pod0_effect_attempts SET state_code=2,observed_at_ms=?1,outcome_schema_version=1,\
         outcome_json=?2,observation_schema_version=1,observation_json=?3 \
         WHERE lease_id=?4 AND fence=?5 AND state_code=1",
        params![input.observed_at_ms, outcome_json, observation_json,
            input.lease.lease_id.into_bytes().as_slice(), fence],
    ).map_err(|error| StorageError::sqlite("stage library network observation", error))?;
    Ok(())
}

fn outcome(action: &LibraryNetworkObservationAction) -> EffectOutcome {
    match action {
        LibraryNetworkObservationAction::Fail { .. } => EffectOutcome::Failed {
            code: pod0_application::ActivityFailureCode::InvalidResponse,
        },
        LibraryNetworkObservationAction::Cancel => EffectOutcome::Cancelled,
        _ => EffectOutcome::Succeeded,
    }
}
