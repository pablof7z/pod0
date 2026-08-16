use pod0_application::{
    RequestDisposition, RequestRejectionReason, UserArtifactMigrationActivityInput,
    UserArtifactMigrationMutation, UserArtifactTransition, plan_user_artifact_migration,
    user_artifact_migration_command_id,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision, UnixTimestampMilliseconds};
use rusqlite::OptionalExtension;
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::note_import_store_support::{current_core_revision, cutover_state};
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_note_cutover(
    path: &std::path::Path,
    observed_at_ms: i64,
) -> Result<bool, StorageError> {
    let receipt = TransitionCommit::open(path)?.commit_resolved_ingress_with(
        UnixTimestampMilliseconds::new(observed_at_ms.max(0)),
        |transaction| {
            let import_id = source_import_id(transaction)?.unwrap_or_else(|| {
                CommandId::from_parts(0, u64::try_from(observed_at_ms.max(0)).unwrap_or_default())
            });
            Ok(TransitionIngress {
                kind: TransitionIngressKind::Migration,
                id: user_artifact_migration_command_id("notes", "commit", import_id).into_bytes(),
                fingerprint: fingerprint(import_id),
            })
        },
        |transaction| {
            let current = current_core_revision(transaction)?;
            let state = cutover_state(transaction)?;
            let disposition = match state.as_deref() {
                Some("staged") => RequestDisposition::Accepted,
                Some("authoritative") => RequestDisposition::AlreadyComplete,
                _ => RequestDisposition::Rejected {
                    reason: RequestRejectionReason::MissingPrerequisite,
                },
            };
            plan_user_artifact_migration(UserArtifactMigrationActivityInput {
                command_id: ingress_command_id(transaction, observed_at_ms)?,
                current_revision: current,
                committed_revision: if disposition == RequestDisposition::Accepted {
                    StateRevision::new(
                        current
                            .value
                            .checked_add(1)
                            .ok_or(StorageError::InvalidActivity)?,
                    )
                } else {
                    current
                },
                transition: UserArtifactTransition::NoteChanged,
                disposition,
                authority_cutover: true,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation {
            UserArtifactMigrationMutation::Apply => {
                require_core_revision(transaction, expected)?;
                let committed = crate::library_store::advance_playback_revision(transaction)?;
                let value =
                    i64::try_from(committed.value).map_err(|_| StorageError::InvalidActivity)?;
                let changed = transaction
                    .execute(
                        "UPDATE pod0_domain_cutovers SET state='authoritative',core_revision=?1,\
                         committed_at_ms=?2 WHERE domain='notes' AND state='staged'",
                        rusqlite::params![value, observed_at_ms],
                    )
                    .map_err(|error| StorageError::sqlite("commit note cutover", error))?;
                (changed == 1)
                    .then_some(committed)
                    .ok_or(StorageError::RevisionConflict)
            }
            UserArtifactMigrationMutation::None => {
                require_core_revision(transaction, expected)?;
                Ok(expected)
            }
        },
    )?;
    match receipt.disposition {
        RequestDisposition::Accepted => Ok(receipt.replayed),
        RequestDisposition::AlreadyComplete | RequestDisposition::Duplicate => Ok(true),
        RequestDisposition::Rejected { .. } => Err(StorageError::ImportNotFound),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn source_import_id(connection: &rusqlite::Connection) -> Result<Option<CommandId>, StorageError> {
    connection
        .query_row(
            "SELECT source_import_id FROM pod0_note_state WHERE singleton=1",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read note cutover import", error))?
        .map(|value| {
            value
                .try_into()
                .map(CommandId::from_bytes)
                .map_err(|_| StorageError::InvalidActivity)
        })
        .transpose()
}

fn ingress_command_id(
    connection: &rusqlite::Connection,
    observed_at_ms: i64,
) -> Result<CommandId, StorageError> {
    let source = source_import_id(connection)?.unwrap_or_else(|| {
        CommandId::from_parts(0, u64::try_from(observed_at_ms.max(0)).unwrap_or_default())
    });
    Ok(user_artifact_migration_command_id(
        "notes", "commit", source,
    ))
}

fn fingerprint(import_id: CommandId) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0-note-cutover-v1");
    hash.update(import_id.into_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}

fn require_core_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (current_core_revision(transaction)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}
