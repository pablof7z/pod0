use pod0_application::{
    RecallConfigurationActivityInput, RecallConfigurationMutation, RequestDisposition,
    plan_recall_configuration_activity,
};
use pod0_domain::{
    CommandId, ContentDigest, RecallConfiguration, RecallConfigurationInput,
    RecallConfigurationOrigin, StateRevision, UnixTimestampMilliseconds,
};

use super::TransitionCommit;
use super::application_support::{fingerprint, legacy_library_receipt, next_core_revision};
use crate::{
    RecallConfigurationMutation as StoredMutation, StorageError, TransitionIngress,
    TransitionIngressKind,
};

pub(crate) fn commit_recall_configuration_import(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    input: RecallConfigurationInput,
    source_generation: ContentDigest,
    observed_at_ms: i64,
) -> Result<StoredMutation, StorageError> {
    commit(
        path,
        command_id,
        command_fingerprint,
        None,
        input,
        Some(source_generation),
        observed_at_ms,
    )
}

pub(crate) fn commit_recall_configuration_set(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    expected_revision: StateRevision,
    input: RecallConfigurationInput,
    observed_at_ms: i64,
) -> Result<StoredMutation, StorageError> {
    commit(
        path,
        command_id,
        command_fingerprint,
        Some(expected_revision),
        input,
        None,
        observed_at_ms,
    )
}

#[allow(clippy::too_many_arguments)]
fn commit(
    path: &std::path::Path,
    command_id: CommandId,
    command_fingerprint: &str,
    expected: Option<StateRevision>,
    input: RecallConfigurationInput,
    source_generation: Option<ContentDigest>,
    observed_at_ms: i64,
) -> Result<StoredMutation, StorageError> {
    let ingress = TransitionIngress {
        kind: TransitionIngressKind::ApplicationCommand,
        id: command_id.into_bytes(),
        fingerprint: fingerprint(command_fingerprint)?,
    };
    let result = std::cell::RefCell::new(None);
    TransitionCommit::open(path)?.commit_planned_with(
        ingress,
        UnixTimestampMilliseconds::new(observed_at_ms.max(0)),
        |transaction| {
            let current = core_revision(transaction)?;
            let committed = next_core_revision(transaction, "read recall configuration revision")?;
            let receipt = legacy_library_receipt(
                transaction,
                command_id,
                command_fingerprint,
                "read recall configuration receipt",
            )?;
            let stored = crate::recall_configuration_store::read_configuration(transaction)?;
            let existing = stored.clone().unwrap_or_default();
            let candidate = RecallConfiguration::validate(
                input.clone(),
                StateRevision::INITIAL,
                if source_generation.is_some() {
                    RecallConfigurationOrigin::LegacySwift
                } else {
                    RecallConfigurationOrigin::User
                },
            )
            .map_err(|_| StorageError::InvalidRecallConfiguration)?;
            let disposition = if receipt.is_some() {
                RequestDisposition::Duplicate
            } else if source_generation.is_some() && stored.is_some() {
                RequestDisposition::NoSemanticChange
            } else if expected.is_some_and(|value| value != existing.revision) {
                return Err(StorageError::RevisionConflict);
            } else if candidate.input() == existing.input() {
                RequestDisposition::NoSemanticChange
            } else {
                RequestDisposition::Accepted
            };
            plan_recall_configuration_activity(RecallConfigurationActivityInput {
                command_id,
                current_revision: current,
                committed_revision: if disposition == RequestDisposition::Accepted {
                    committed
                } else {
                    current
                },
                disposition,
                migration: source_generation.is_some(),
            })
            .map(|plan| plan.map_mutation(|mutation| (mutation, stored, candidate, committed)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected_core, (mutation, stored, candidate, committed)| {
            let configuration = match mutation {
                RecallConfigurationMutation::Apply => {
                    require_core_revision(transaction, expected_core)?;
                    let configuration = if source_generation.is_some() {
                        RecallConfiguration::legacy_or_default(input.clone(), committed)
                    } else {
                        RecallConfiguration::validate(
                            candidate.input(),
                            committed,
                            RecallConfigurationOrigin::User,
                        )
                        .map_err(|_| StorageError::InvalidRecallConfiguration)?
                    };
                    if stored.is_some() {
                        crate::recall_configuration_store::update_configuration(
                            transaction,
                            &configuration,
                            observed_at_ms,
                        )?;
                    } else {
                        crate::recall_configuration_store::insert_configuration(
                            transaction,
                            &configuration,
                            source_generation,
                            observed_at_ms,
                        )?;
                    }
                    let actual = crate::library_store::finish_command(
                        transaction,
                        command_id,
                        command_fingerprint,
                        observed_at_ms,
                    )?;
                    if actual != committed {
                        return Err(StorageError::RevisionConflict);
                    }
                    configuration
                }
                RecallConfigurationMutation::None => stored.unwrap_or_default(),
            };
            *result.borrow_mut() = Some(StoredMutation {
                changed: mutation == RecallConfigurationMutation::Apply,
                imported: mutation == RecallConfigurationMutation::Apply
                    && source_generation.is_some(),
                configuration,
            });
            Ok(if mutation == RecallConfigurationMutation::Apply {
                committed
            } else {
                expected_core
            })
        },
    )?;
    result.into_inner().ok_or(StorageError::InvalidActivity)
}

fn core_revision(connection: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read recall configuration core revision", error))?;
    Ok(StateRevision::new(
        u64::try_from(value).map_err(|_| StorageError::InvalidActivity)?,
    ))
}

fn require_core_revision(
    connection: &rusqlite::Connection,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (core_revision(connection)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}
