use pod0_application::{
    RequestDisposition, RequestRejectionReason, UserArtifactMigrationActivityInput,
    UserArtifactMigrationMutation, UserArtifactTransition, plan_user_artifact_migration,
    user_artifact_migration_command_id,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision, UnixTimestampMilliseconds};
use rusqlite::OptionalExtension;
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::clip_import_store_support::{current_core_revision, cutover_state};
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_clip_cutover(
    source_path: &std::path::Path,
    path: &std::path::Path,
    observed_at_ms: i64,
) -> Result<bool, StorageError> {
    let receipt = TransitionCommit::open(path)?.commit_resolved_ingress_with(
        UnixTimestampMilliseconds::new(observed_at_ms.max(0)),
        |transaction| ingress(transaction, observed_at_ms),
        |transaction| {
            let current = current_core_revision(transaction)?;
            let state = cutover_state(transaction)?;
            let (disposition, discard_staged) = match state.as_deref() {
                Some("staged") => {
                    let current_source =
                        crate::legacy_clip_source::inspect_clip_source(source_path)?;
                    validate_stage(transaction, &current_source)?
                }
                Some("authoritative") => (RequestDisposition::AlreadyComplete, false),
                _ => (
                    RequestDisposition::Rejected {
                        reason: RequestRejectionReason::MissingPrerequisite,
                    },
                    false,
                ),
            };
            let command_id = migration_command_id(transaction, observed_at_ms)?;
            plan_user_artifact_migration(UserArtifactMigrationActivityInput {
                command_id,
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
                transition: UserArtifactTransition::ClipChanged,
                disposition,
                authority_cutover: true,
            })
            .map(|plan| plan.map_mutation(|mutation| (mutation, discard_staged)))
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, (mutation, discard_staged)| match mutation {
            UserArtifactMigrationMutation::Apply => {
                require_core_revision(transaction, expected)?;
                let committed = crate::library_store::advance_playback_revision(transaction)?;
                let value =
                    i64::try_from(committed.value).map_err(|_| StorageError::InvalidActivity)?;
                let changed = transaction
                    .execute(
                        "UPDATE pod0_domain_cutovers SET state='authoritative',core_revision=?1,\
                         committed_at_ms=?2 WHERE domain='clips' AND state='staged'",
                        rusqlite::params![value, observed_at_ms],
                    )
                    .map_err(|error| StorageError::sqlite("commit clip cutover", error))?;
                (changed == 1)
                    .then_some(committed)
                    .ok_or(StorageError::RevisionConflict)
            }
            UserArtifactMigrationMutation::None => {
                require_core_revision(transaction, expected)?;
                if discard_staged {
                    crate::clip_import_store::discard_staged_clip_import(transaction)?;
                }
                Ok(expected)
            }
        },
    )?;
    match receipt.disposition {
        RequestDisposition::Accepted => Ok(receipt.replayed),
        RequestDisposition::AlreadyComplete | RequestDisposition::Duplicate => Ok(true),
        RequestDisposition::Rejected {
            reason: RequestRejectionReason::RevisionConflict,
        } => Err(StorageError::SourceChanged),
        RequestDisposition::Rejected { .. } => Err(StorageError::ImportNotFound),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn validate_stage(
    transaction: &rusqlite::Transaction<'_>,
    current: &crate::InspectedClipSource,
) -> Result<(RequestDisposition, bool), StorageError> {
    let import_id = source_import_id(transaction)?.ok_or(StorageError::ImportNotFound)?;
    let report =
        crate::clip_import_store_support::stored_clip_import_report(transaction, import_id, None)?
            .ok_or(StorageError::ImportNotFound)?;
    let snapshot = crate::clip_store_read::read_clip_snapshot(transaction)?;
    if snapshot.clips.len() != report.plan.clip_count as usize
        || crate::legacy_clip_source::digest(&snapshot.clips) != report.plan.source_hash
    {
        return Err(StorageError::CorruptSchema {
            detail: "staged clip snapshot does not match its verified import",
        });
    }
    if current.plan.source_kind != report.plan.source_kind
        || current.plan.source_hash != report.plan.source_hash
        || current.clips != snapshot.clips
    {
        return Ok((
            RequestDisposition::Rejected {
                reason: RequestRejectionReason::RevisionConflict,
            },
            true,
        ));
    }
    Ok((RequestDisposition::Accepted, false))
}

fn source_import_id(connection: &rusqlite::Connection) -> Result<Option<CommandId>, StorageError> {
    connection
        .query_row(
            "SELECT source_import_id FROM pod0_clip_state WHERE singleton=1",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read clip cutover import", error))?
        .map(|value| crate::model::command_id(&value))
        .transpose()
}

fn migration_command_id(
    connection: &rusqlite::Connection,
    observed_at_ms: i64,
) -> Result<CommandId, StorageError> {
    let source = source_import_id(connection)?.unwrap_or_else(|| {
        CommandId::from_parts(0, u64::try_from(observed_at_ms.max(0)).unwrap_or_default())
    });
    Ok(user_artifact_migration_command_id(
        "clips", "commit", source,
    ))
}

fn ingress(
    transaction: &rusqlite::Transaction<'_>,
    observed_at_ms: i64,
) -> Result<TransitionIngress, StorageError> {
    let command_id = migration_command_id(transaction, observed_at_ms)?;
    let mut hash = Sha256::new();
    hash.update(b"pod0-clip-cutover-v1");
    hash.update(command_id.into_bytes());
    Ok(TransitionIngress {
        kind: TransitionIngressKind::Migration,
        id: command_id.into_bytes(),
        fingerprint: ContentDigest::from_bytes(hash.finalize().into()),
    })
}

fn require_core_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (current_core_revision(transaction)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}
