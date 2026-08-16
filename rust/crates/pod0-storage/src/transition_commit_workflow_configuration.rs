use pod0_application::{
    RequestDisposition, WORKFLOW_CONFIGURATION_SCHEMA_VERSION, WorkflowCapabilitySnapshot,
    WorkflowConfiguration, WorkflowConfigurationActivityInput, WorkflowConfigurationActivityKind,
    WorkflowConfigurationInput, WorkflowConfigurationMutation, WorkflowConfigurationOrigin,
    plan_workflow_configuration_activity,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision, UnixTimestampMilliseconds};

use super::TransitionCommit;
use crate::{
    LibraryStore, StorageError, TransitionIngress, TransitionIngressKind,
    WorkflowCapabilityCommitOutcome, WorkflowConfigurationCommitOutcome,
};

pub(crate) fn commit_workflow_configuration_import(
    path: &std::path::Path,
    command_id: CommandId,
    fingerprint: ContentDigest,
    input: WorkflowConfigurationInput,
    source_generation: ContentDigest,
    observed_at_ms: i64,
) -> Result<WorkflowConfigurationCommitOutcome, StorageError> {
    commit_configuration(
        path,
        command_id,
        fingerprint,
        input,
        None,
        Some(source_generation),
        observed_at_ms,
    )
}

pub(crate) fn commit_workflow_configuration_set(
    path: &std::path::Path,
    command_id: CommandId,
    fingerprint: ContentDigest,
    expected_revision: StateRevision,
    input: WorkflowConfigurationInput,
    observed_at_ms: i64,
) -> Result<WorkflowConfigurationCommitOutcome, StorageError> {
    commit_configuration(
        path,
        command_id,
        fingerprint,
        input,
        Some(expected_revision),
        None,
        observed_at_ms,
    )
}

#[allow(clippy::too_many_arguments)]
fn commit_configuration(
    path: &std::path::Path,
    command_id: CommandId,
    fingerprint: ContentDigest,
    input: WorkflowConfigurationInput,
    expected_revision: Option<StateRevision>,
    source_generation: Option<ContentDigest>,
    observed_at_ms: i64,
) -> Result<WorkflowConfigurationCommitOutcome, StorageError> {
    let import = source_generation.is_some();
    let timestamp = UnixTimestampMilliseconds::new(observed_at_ms.max(0));
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        ingress(command_id, fingerprint),
        timestamp,
        |transaction| {
            let current = current_revision(transaction)?;
            let stored = crate::workflow_configuration_store::read_configuration(transaction)?;
            let disposition = if input.validate().is_err() {
                RequestDisposition::Rejected {
                    reason: pod0_application::RequestRejectionReason::Invalid,
                }
            } else if import && stored.is_some() {
                RequestDisposition::NoSemanticChange
            } else if expected_revision.is_some_and(|expected| {
                stored.as_ref().map(|value| value.revision) != Some(expected)
            }) {
                RequestDisposition::Rejected {
                    reason: pod0_application::RequestRejectionReason::RevisionConflict,
                }
            } else if stored.as_ref().is_some_and(|value| value.value == input) {
                RequestDisposition::NoSemanticChange
            } else {
                RequestDisposition::Accepted
            };
            let committed = if disposition == RequestDisposition::Accepted {
                next_revision(current)?
            } else {
                current
            };
            plan_workflow_configuration_activity(WorkflowConfigurationActivityInput {
                command_id,
                current_revision: current,
                committed_revision: committed,
                disposition,
                kind: if import {
                    WorkflowConfigurationActivityKind::ImportAuthority
                } else {
                    WorkflowConfigurationActivityKind::Set
                },
            })
            .map(|plan| plan.map_mutation(|mutation| (mutation, committed)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, current, (mutation, committed)| {
            if mutation == WorkflowConfigurationMutation::None {
                return Ok(current);
            }
            require_revision(transaction, current)?;
            crate::workflow_configuration_store::write_configuration(
                transaction,
                &WorkflowConfiguration {
                    schema_version: WORKFLOW_CONFIGURATION_SCHEMA_VERSION,
                    revision: committed,
                    origin: if import {
                        WorkflowConfigurationOrigin::LegacySwiftImport
                    } else {
                        WorkflowConfigurationOrigin::User
                    },
                    value: input.clone(),
                },
                source_generation,
                timestamp.value,
            )?;
            advance_revision(transaction, committed)
        },
    )?;
    let configuration = LibraryStore::open_authoritative(path)?.workflow_configuration()?;
    Ok(WorkflowConfigurationCommitOutcome {
        configuration,
        changed: receipt.disposition == RequestDisposition::Accepted && !receipt.replayed,
        imported: import && receipt.disposition == RequestDisposition::Accepted,
        receipt,
    })
}

pub(crate) fn commit_workflow_capabilities(
    path: &std::path::Path,
    command_id: CommandId,
    fingerprint: ContentDigest,
    snapshot: WorkflowCapabilitySnapshot,
) -> Result<WorkflowCapabilityCommitOutcome, StorageError> {
    let timestamp = snapshot.observed_at;
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        ingress(command_id, fingerprint),
        timestamp,
        |transaction| {
            let current = current_revision(transaction)?;
            let authority = crate::workflow_configuration_store::read_configuration(transaction)?;
            let stored =
                crate::workflow_configuration_store::read_capability_snapshot(transaction)?;
            let disposition = if authority.is_none() {
                RequestDisposition::Rejected {
                    reason: pod0_application::RequestRejectionReason::MissingPrerequisite,
                }
            } else if stored
                .as_ref()
                .is_some_and(|value| value.snapshot_id == snapshot.snapshot_id)
            {
                RequestDisposition::NoSemanticChange
            } else {
                RequestDisposition::Accepted
            };
            let committed = if disposition == RequestDisposition::Accepted {
                next_revision(current)?
            } else {
                current
            };
            plan_workflow_configuration_activity(WorkflowConfigurationActivityInput {
                command_id,
                current_revision: current,
                committed_revision: committed,
                disposition,
                kind: WorkflowConfigurationActivityKind::ObserveCapabilities,
            })
            .map(|plan| plan.map_mutation(|mutation| (mutation, committed)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, current, (mutation, committed)| {
            if mutation == WorkflowConfigurationMutation::None {
                return Ok(current);
            }
            require_revision(transaction, current)?;
            crate::workflow_configuration_store::write_capability_snapshot(
                transaction,
                &snapshot,
                committed,
            )?;
            advance_revision(transaction, committed)
        },
    )?;
    let snapshot = LibraryStore::open_authoritative(path)?.workflow_capability_snapshot()?;
    Ok(WorkflowCapabilityCommitOutcome {
        snapshot,
        changed: receipt.disposition == RequestDisposition::Accepted && !receipt.replayed,
        receipt,
    })
}

fn ingress(command_id: CommandId, fingerprint: ContentDigest) -> TransitionIngress {
    TransitionIngress {
        kind: TransitionIngressKind::ApplicationCommand,
        id: command_id.into_bytes(),
        fingerprint,
    }
}

fn current_revision(connection: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read workflow configuration revision", error))?;
    Ok(StateRevision::new(
        u64::try_from(value).map_err(|_| StorageError::InvalidActivity)?,
    ))
}

fn next_revision(current: StateRevision) -> Result<StateRevision, StorageError> {
    current
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(StorageError::InvalidActivity)
}

fn require_revision(
    connection: &rusqlite::Connection,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (current_revision(connection)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}

fn advance_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<StateRevision, StorageError> {
    let actual = crate::library_store::advance_playback_revision(transaction)?;
    (actual == expected)
        .then_some(actual)
        .ok_or(StorageError::RevisionConflict)
}
