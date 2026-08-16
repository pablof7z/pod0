use pod0_application::{
    RequestDisposition, RequestRejectionReason, UserArtifactMigrationActivityInput,
    UserArtifactMigrationMutation, UserArtifactTransition, plan_user_artifact_migration,
    user_artifact_migration_command_id,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use super::TransitionCommit;
use crate::{MemoryCutoverState, StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_memory_cutover(
    path: &std::path::Path,
    source_generation: u64,
    observed_at_ms: i64,
) -> Result<crate::LegacyMemoryCutoverReport, StorageError> {
    let command_id = migration_command_id(source_generation);
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::Migration,
            id: command_id.into_bytes(),
            fingerprint: fingerprint(command_id, source_generation),
        },
        UnixTimestampMilliseconds::new(observed_at_ms.max(0)),
        |transaction| {
            let current = core_revision(transaction)?;
            let report =
                crate::memory_cutover_store::matching_report(transaction, source_generation)?;
            let disposition = match report.state {
                MemoryCutoverState::Authoritative { .. } => RequestDisposition::AlreadyComplete,
                MemoryCutoverState::Verified { .. } => {
                    crate::memory_cutover_store::verify_rows(transaction, &report)?;
                    RequestDisposition::Accepted
                }
                _ => RequestDisposition::Rejected {
                    reason: RequestRejectionReason::MissingPrerequisite,
                },
            };
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
                transition: UserArtifactTransition::MemoryChanged,
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
                        "UPDATE pod0_memory_state SET authority_active=1,collection_revision=?1 \
                         WHERE singleton=1 AND authority_active=0",
                        [value],
                    )
                    .map_err(|error| StorageError::sqlite("commit memory authority", error))?;
                if changed != 1 {
                    return Err(StorageError::RevisionConflict);
                }
                transaction
                    .execute(
                        "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,\
                         core_revision,committed_at_ms) \
                         VALUES('memories','authoritative',?1,?2,?3)",
                        rusqlite::params![
                            crate::memory_cutover_store::to_i64(source_generation)?,
                            value,
                            observed_at_ms
                        ],
                    )
                    .map_err(|error| StorageError::sqlite("commit memory cutover marker", error))?;
                let evidence = transaction
                    .execute(
                        "UPDATE pod0_memory_cutover_evidence SET state='authoritative',\
                         committed_at_ms=?1 WHERE singleton=1 AND state='verified'",
                        [observed_at_ms],
                    )
                    .map_err(|error| StorageError::sqlite("commit memory evidence", error))?;
                (evidence == 1)
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
        RequestDisposition::Accepted
        | RequestDisposition::AlreadyComplete
        | RequestDisposition::Duplicate => {
            crate::LibraryStore::open_authoritative(path)?.memory_cutover_report()
        }
        RequestDisposition::Rejected { .. } => Err(StorageError::RevisionConflict),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn migration_command_id(source_generation: u64) -> CommandId {
    user_artifact_migration_command_id(
        "memories",
        "commit",
        CommandId::from_parts(0, source_generation),
    )
}

fn fingerprint(command_id: CommandId, source_generation: u64) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0-memory-cutover-v1");
    hash.update(command_id.into_bytes());
    hash.update(source_generation.to_be_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}

fn core_revision(transaction: &rusqlite::Transaction<'_>) -> Result<StateRevision, StorageError> {
    let value: i64 = transaction
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read memory cutover revision", error))?;
    super::application_support::revision(value)
}

fn require_core_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (core_revision(transaction)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}
