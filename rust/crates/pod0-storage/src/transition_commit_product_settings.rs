use pod0_application::{SettingsChange, SettingsMutation, plan_settings_transition};
use pod0_domain::{CommandId, ContentDigest, StateRevision, UnixTimestampMilliseconds};

use super::TransitionCommit;
use crate::{
    LibraryStore, SettingsCommitOutcome, StorageError, TransitionIngress, TransitionIngressKind,
};

pub(crate) fn commit_product_settings_change(
    path: &std::path::Path,
    command_id: CommandId,
    fingerprint: ContentDigest,
    change: SettingsChange,
    observed_at_ms: i64,
) -> Result<SettingsCommitOutcome, StorageError> {
    let timestamp = UnixTimestampMilliseconds::new(observed_at_ms.max(0));
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: command_id.into_bytes(),
            fingerprint,
        },
        timestamp,
        |transaction| {
            let revision = current_revision(transaction)?;
            let current = crate::product_settings_store::read_settings(transaction)?;
            plan_settings_transition(command_id, revision, current.as_ref(), change)
                .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, current, mutation| {
            commit_mutation(transaction, command_id, current, mutation, timestamp.value)
        },
    )?;
    let store = LibraryStore::open_authoritative(path)?;
    Ok(SettingsCommitOutcome {
        settings: store.product_settings()?,
        validation: store.read(|connection| {
            crate::product_settings_store::read_validation(connection, command_id)
        })?,
        conflict: store.read(|connection| {
            crate::product_settings_store::read_conflict(connection, command_id)
        })?,
        changed: receipt.disposition == pod0_application::RequestDisposition::Accepted
            && !receipt.replayed,
        receipt,
    })
}

fn commit_mutation(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    current: StateRevision,
    mutation: SettingsMutation,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    crate::product_settings_store::write_validation(
        transaction,
        command_id,
        mutation.source,
        mutation.candidate_schema_version,
        mutation.candidate_version,
        mutation.validation,
        observed_at_ms,
    )?;
    if let Some(conflict) = mutation.conflict {
        crate::product_settings_store::write_conflict(
            transaction,
            command_id,
            conflict,
            observed_at_ms,
        )?;
    }
    let Some(next) = mutation.next else {
        return Ok(current);
    };
    require_revision(transaction, current)?;
    crate::product_settings_store::write_settings(transaction, &next, observed_at_ms)?;
    let revision = crate::library_store::advance_playback_revision(transaction)?;
    (revision == next.revision)
        .then_some(revision)
        .ok_or(StorageError::RevisionConflict)
}

fn current_revision(connection: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read product settings revision", error))?;
    Ok(StateRevision::new(
        u64::try_from(value).map_err(|_| StorageError::InvalidActivity)?,
    ))
}

fn require_revision(
    connection: &rusqlite::Connection,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (current_revision(connection)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}
