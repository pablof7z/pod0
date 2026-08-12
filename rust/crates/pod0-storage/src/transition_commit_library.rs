use pod0_application::{EpisodeStarredMutation, EpisodeStarredState, plan_episode_starred};
use pod0_domain::{CommandId, ContentDigest, EpisodeId, StateRevision, UnixTimestampMilliseconds};
use rusqlite::{OptionalExtension, Transaction, params};

use super::TransitionCommit;
use crate::library_store::finish_command;
use crate::migration_db::{open_connection, user_version, validate_current_database_identity};
use crate::{StorageError, TransitionIngress, TransitionIngressKind};

pub(crate) fn commit_episode_starred(
    path: &std::path::Path,
    command_id: CommandId,
    fingerprint: &str,
    episode_id: EpisodeId,
    starred: bool,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let connection = open_connection(path, true)?;
    validate_current_database_identity(&connection, user_version(&connection)?)?;
    let (stored_starred, revision): (bool, i64) = connection
        .query_row(
            "SELECT e.is_starred,p.state_revision FROM pod0_episodes e \
             CROSS JOIN pod0_playback_state p WHERE e.episode_id=?1 AND p.singleton=1",
            [episode_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read episode starred transition state", error))?
        .ok_or(StorageError::EntityNotFound)?;
    let legacy = connection
        .query_row(
            "SELECT command_fingerprint,applied_revision FROM pod0_library_commands \
             WHERE command_id=?1",
            [command_id.into_bytes().as_slice()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read legacy library receipt", error))?;
    let legacy_revision = match legacy {
        Some((stored, revision)) if stored == fingerprint => Some(revision_value(revision)?),
        Some(_) => return Err(StorageError::CommandConflict),
        None => None,
    };
    drop(connection);
    let revision = revision_value(revision)?;
    let plan = plan_episode_starred(
        command_id,
        EpisodeStarredState {
            episode_id,
            starred: stored_starred,
            revision,
            legacy_command_revision: legacy_revision,
        },
        starred,
    )
    .map_err(|_| StorageError::InvalidActivity)?;
    let fingerprint_digest = fingerprint_digest(fingerprint)?;
    let receipt = TransitionCommit::open(path)?.commit_with(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: command_id.into_bytes(),
            fingerprint: fingerprint_digest,
        },
        plan,
        UnixTimestampMilliseconds::new(observed_at_ms),
        |transaction, expected, mutation| match mutation {
            EpisodeStarredMutation::Set {
                episode_id,
                starred,
            } => apply_starred(
                transaction,
                command_id,
                fingerprint,
                episode_id,
                starred,
                expected,
                observed_at_ms,
            ),
            EpisodeStarredMutation::RecordNoChange => {
                require_revision(transaction, expected)?;
                record_no_change(
                    transaction,
                    command_id,
                    fingerprint,
                    expected,
                    observed_at_ms,
                )?;
                Ok(expected)
            }
            EpisodeStarredMutation::LegacyDuplicate => Ok(legacy_revision.expect("planned legacy")),
        },
    )?;
    Ok(receipt.committed_revision)
}

#[allow(clippy::too_many_arguments)]
fn apply_starred(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    episode_id: EpisodeId,
    starred: bool,
    expected: StateRevision,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    require_revision(transaction, expected)?;
    let changed = transaction
        .execute(
            "UPDATE pod0_episodes SET is_starred=?1 WHERE episode_id=?2",
            params![starred, episode_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("update episode starred state", error))?;
    if changed != 1 {
        return Err(StorageError::EntityNotFound);
    }
    finish_command(transaction, command_id, fingerprint, observed_at_ms)
}

fn require_revision(
    transaction: &Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    let current: i64 = transaction
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read transition revision", error))?;
    if revision_value(current)? == expected {
        Ok(())
    } else {
        Err(StorageError::RevisionConflict)
    }
}

fn record_no_change(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    revision: StateRevision,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let revision = i64::try_from(revision.value).map_err(|_| StorageError::InvalidActivity)?;
    transaction
        .execute(
            "INSERT INTO pod0_library_commands(command_id,command_fingerprint,applied_revision,\
         completed_at_ms) VALUES(?1,?2,?3,?4)",
            params![
                command_id.into_bytes().as_slice(),
                fingerprint,
                revision,
                observed_at_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("record no-change library receipt", error))?;
    Ok(())
}

fn revision_value(value: i64) -> Result<StateRevision, StorageError> {
    Ok(StateRevision::new(u64::try_from(value).map_err(|_| {
        StorageError::CorruptSchema {
            detail: "core revision is malformed",
        }
    })?))
}

fn fingerprint_digest(value: &str) -> Result<ContentDigest, StorageError> {
    if value.len() != 64 {
        return Err(StorageError::CommandConflict);
    }
    let mut bytes = [0_u8; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| StorageError::CommandConflict)?;
    }
    Ok(ContentDigest::from_bytes(bytes))
}
