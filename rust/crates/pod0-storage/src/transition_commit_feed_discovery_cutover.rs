use pod0_application::{
    LibraryFeedMigrationInput, LibraryFeedMigrationMutation, LibraryFeedTransition,
    RequestDisposition, plan_library_feed_migration,
};
use pod0_domain::{CommandId, StateRevision, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use crate::feed_discovery_cutover_commit::commit_cutover;
use crate::feed_discovery_cutover_read::read_report;
use crate::transition_commit::TransitionCommit;
use crate::{FeedDiscoveryCutoverState, StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_feed_discovery_authority_cutover(
    path: &std::path::Path,
    source_generation: u64,
    observed_at: UnixTimestampMilliseconds,
) -> Result<(), StorageError> {
    TransitionCommit::open(path)?.commit_resolved_ingress_with(
        observed_at,
        |transaction| {
            let report = read_report(transaction)?;
            let fingerprint = report
                .source_fingerprint
                .ok_or(StorageError::FeedDiscoveryCutoverConflict)?;
            Ok(TransitionIngress {
                kind: TransitionIngressKind::Migration,
                id: feed_cutover_migration_id(source_generation).into_bytes(),
                fingerprint,
            })
        },
        |transaction| {
            let report = read_report(transaction)?;
            let disposition = match report.state {
                FeedDiscoveryCutoverState::Staged {
                    source_generation: existing,
                } if existing == source_generation => RequestDisposition::Accepted,
                FeedDiscoveryCutoverState::Authoritative {
                    source_generation: existing,
                } if existing == source_generation => RequestDisposition::AlreadyComplete,
                _ => return Err(StorageError::FeedDiscoveryCutoverConflict),
            };
            plan_library_feed_migration(LibraryFeedMigrationInput {
                migration_id: feed_cutover_migration_id(source_generation),
                current_revision: library_revision(transaction)?,
                transition: LibraryFeedTransition::FeedDiscoveryAuthorityChanged,
                disposition,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation {
            LibraryFeedMigrationMutation::Apply => {
                let committed_revision = StateRevision::new(
                    expected
                        .value
                        .checked_add(1)
                        .ok_or(StorageError::InvalidActivity)?,
                );
                commit_cutover(
                    transaction,
                    source_generation,
                    observed_at,
                    committed_revision,
                )?;
                crate::library_store::advance_playback_revision(transaction)
            }
            LibraryFeedMigrationMutation::None => Ok(expected),
        },
    )?;
    Ok(())
}

pub(crate) fn feed_cutover_migration_id(source_generation: u64) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-feed-discovery-authority-cutover-v1\0");
    hash.update(source_generation.to_be_bytes());
    CommandId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}

fn library_revision(connection: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read feed cutover revision", error))?;
    Ok(StateRevision::new(
        u64::try_from(value).map_err(|_| StorageError::InvalidActivity)?,
    ))
}
