use std::path::Path;

use pod0_application::{
    LibraryFeedMigrationInput, LibraryFeedMigrationMutation, LibraryFeedTransition,
    RequestDisposition, plan_library_feed_migration,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision, UnixTimestampMilliseconds};
use rusqlite::OptionalExtension;
use sha2::{Digest as _, Sha256};

use crate::transition_commit::TransitionCommit;
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_listening_authority_cutover(
    path: &Path,
    observed_at_ms: i64,
) -> Result<bool, StorageError> {
    let receipt = TransitionCommit::open(path)?.commit_resolved_ingress_with(
        UnixTimestampMilliseconds::new(observed_at_ms),
        |transaction| {
            let source_id = listening_source_id(transaction)?;
            Ok(TransitionIngress {
                kind: TransitionIngressKind::Migration,
                id: migration_id(b"listening", source_id).into_bytes(),
                fingerprint: migration_fingerprint(b"listening", source_id),
            })
        },
        |transaction| {
            let source_id = listening_source_id(transaction)?;
            let disposition = match listening_cutover_state(transaction)?.as_deref() {
                Some("staged") => RequestDisposition::Accepted,
                Some("authoritative") => RequestDisposition::AlreadyComplete,
                _ => return Err(StorageError::ImportNotFound),
            };
            plan_library_feed_migration(LibraryFeedMigrationInput {
                migration_id: migration_id(b"listening", source_id),
                current_revision: listening_revision(transaction)?,
                transition: LibraryFeedTransition::ListeningAuthorityChanged,
                disposition,
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation {
            LibraryFeedMigrationMutation::Apply => {
                let changed = transaction
                    .execute(
                        "UPDATE pod0_domain_cutovers SET state='authoritative',committed_at_ms=?1 \
                         WHERE domain='listening' AND state='staged'",
                        [observed_at_ms],
                    )
                    .map_err(|error| StorageError::sqlite("commit listening authority", error))?;
                if changed != 1 {
                    return Err(StorageError::RevisionConflict);
                }
                crate::library_store::advance_playback_revision(transaction)
            }
            LibraryFeedMigrationMutation::None => Ok(expected),
        },
    )?;
    Ok(receipt.replayed || receipt.disposition == RequestDisposition::AlreadyComplete)
}

fn listening_source_id(connection: &rusqlite::Connection) -> Result<CommandId, StorageError> {
    let bytes: Vec<u8> = connection
        .query_row(
            "SELECT source_import_id FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read listening migration source", error))?
        .ok_or(StorageError::ImportNotFound)?;
    Ok(CommandId::from_bytes(
        bytes
            .try_into()
            .map_err(|_| StorageError::InvalidActivity)?,
    ))
}

fn listening_cutover_state(
    connection: &rusqlite::Connection,
) -> Result<Option<String>, StorageError> {
    connection
        .query_row(
            "SELECT state FROM pod0_domain_cutovers WHERE domain='listening'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read listening cutover", error))
}

fn listening_revision(connection: &rusqlite::Connection) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read listening migration revision", error))?;
    Ok(StateRevision::new(
        u64::try_from(value).map_err(|_| StorageError::InvalidActivity)?,
    ))
}

pub(crate) fn migration_id(domain: &[u8], source_id: CommandId) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-library-feed-authority-migration-v1\0");
    hash.update(domain);
    hash.update(source_id.into_bytes());
    CommandId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}

fn migration_fingerprint(domain: &[u8], source_id: CommandId) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0-library-feed-authority-migration-fingerprint-v1\0");
    hash.update(domain);
    hash.update(source_id.into_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}
