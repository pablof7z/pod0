use std::path::{Path, PathBuf};

use pod0_domain::{CommandId, EpisodeId, ListeningDomainSnapshot, StateRevision};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::listening_store_read::read_snapshot;
use crate::migration_db::{configure, open_connection, user_version, validate_open_database};
use crate::{CURRENT_SCHEMA_VERSION, StorageError};

#[derive(Clone, Debug)]
pub struct LibraryStore {
    path: PathBuf,
}

impl LibraryStore {
    pub fn open_authoritative(path: &Path) -> Result<Self, StorageError> {
        let connection = open_current(path, true)?;
        require_authoritative(&connection)?;
        Ok(Self {
            path: path.to_owned(),
        })
    }

    pub fn snapshot(&self) -> Result<ListeningDomainSnapshot, StorageError> {
        let connection = open_current(&self.path, true)?;
        require_authoritative(&connection)?;
        read_snapshot(&connection)
    }

    pub fn note_snapshot(&self) -> Result<crate::NoteCollectionSnapshot, StorageError> {
        let connection = open_current(&self.path, true)?;
        require_authoritative(&connection)?;
        crate::note_store_read::require_notes_authoritative(&connection)?;
        crate::note_store_read::read_note_snapshot(&connection)
    }

    pub fn clip_snapshot(&self) -> Result<crate::ClipCollectionSnapshot, StorageError> {
        let connection = open_current(&self.path, true)?;
        require_authoritative(&connection)?;
        crate::clip_store_read::require_clips_authoritative(&connection)?;
        crate::clip_store_read::read_clip_snapshot(&connection)
    }

    pub fn selected_chapter_artifact(
        &self,
        episode_id: EpisodeId,
    ) -> Result<Option<crate::SelectedChapterArtifact>, StorageError> {
        let connection = open_current(&self.path, true)?;
        require_authoritative(&connection)?;
        crate::chapter_authority::require_chapter_authoritative(&connection)?;
        crate::chapter_store_read_selection::read_selected_chapter_artifact(&connection, episode_id)
    }

    pub fn require_clips_authoritative(&self) -> Result<(), StorageError> {
        let connection = open_current(&self.path, true)?;
        require_authoritative(&connection)?;
        crate::clip_store_read::require_clips_authoritative(&connection)
    }

    pub fn require_notes_authoritative(&self) -> Result<(), StorageError> {
        let connection = open_current(&self.path, true)?;
        require_authoritative(&connection)?;
        crate::note_store_read::require_notes_authoritative(&connection)
    }

    pub(crate) fn write<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let mut connection = open_current(&self.path, false)?;
        configure(&connection)?;
        require_authoritative(&connection)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| StorageError::sqlite("begin library command", error))?;
        let output = operation(&transaction)?;
        transaction
            .commit()
            .map_err(|error| StorageError::sqlite("commit library command", error))?;
        Ok(output)
    }
}

pub fn commit_listening_cutover(path: &Path, observed_at_ms: i64) -> Result<bool, StorageError> {
    let mut connection = open_current(path, false)?;
    configure(&connection)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StorageError::sqlite("begin listening cutover", error))?;
    let state: Option<String> = transaction
        .query_row(
            "SELECT state FROM pod0_domain_cutovers WHERE domain='listening'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read listening cutover", error))?;
    let was_already_authoritative = match state.as_deref() {
        Some("authoritative") => true,
        Some("staged") => {
            transaction
                .execute(
                    "UPDATE pod0_domain_cutovers SET state='authoritative',committed_at_ms=?1 \
                     WHERE domain='listening' AND state='staged'",
                    [observed_at_ms],
                )
                .map_err(|error| StorageError::sqlite("commit listening cutover", error))?;
            false
        }
        _ => return Err(StorageError::ImportNotFound),
    };
    transaction
        .commit()
        .map_err(|error| StorageError::sqlite("commit listening cutover", error))?;
    Ok(was_already_authoritative)
}

pub(crate) fn command_was_applied(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
) -> Result<Option<StateRevision>, StorageError> {
    let existing: Option<(String, i64)> = transaction
        .query_row(
            "SELECT command_fingerprint,applied_revision FROM pod0_library_commands \
             WHERE command_id=?1",
            [command_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read library command receipt", error))?;
    match existing {
        Some((stored, revision)) if stored == fingerprint => Ok(Some(StateRevision::new(
            u64::try_from(revision).map_err(|_| StorageError::CorruptSchema {
                detail: "library command revision is malformed",
            })?,
        ))),
        Some(_) => Err(StorageError::CommandConflict),
        None => Ok(None),
    }
}

pub(crate) fn finish_command(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let current: i64 = transaction
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read core revision", error))?;
    let next = current.checked_add(1).ok_or(StorageError::CorruptSchema {
        detail: "core revision exhausted",
    })?;
    transaction
        .execute(
            "UPDATE pod0_playback_state SET state_revision=?1 WHERE singleton=1",
            [next],
        )
        .map_err(|error| StorageError::sqlite("advance core revision", error))?;
    transaction
        .execute(
            "UPDATE pod0_domain_cutovers SET core_revision=?1 WHERE domain='listening'",
            [next],
        )
        .map_err(|error| StorageError::sqlite("advance cutover revision", error))?;
    transaction
        .execute(
            "INSERT INTO pod0_library_commands(command_id,command_fingerprint,applied_revision,\
             completed_at_ms) VALUES(?1,?2,?3,?4)",
            params![
                command_id.into_bytes().as_slice(),
                fingerprint,
                next,
                observed_at_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("record library command receipt", error))?;
    Ok(StateRevision::new(u64::try_from(next).map_err(|_| {
        StorageError::CorruptSchema {
            detail: "core revision is malformed",
        }
    })?))
}

pub(crate) fn source_import_id(transaction: &Transaction<'_>) -> Result<Vec<u8>, StorageError> {
    transaction
        .query_row(
            "SELECT import_id FROM pod0_listening_imports ORDER BY verified_at_ms LIMIT 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read library import origin", error))
}

fn open_current(path: &Path, read_only: bool) -> Result<Connection, StorageError> {
    let connection = open_connection(path, read_only)?;
    let version = user_version(&connection)?;
    validate_open_database(&connection, version)?;
    if version != CURRENT_SCHEMA_VERSION {
        return Err(StorageError::CorruptSchema {
            detail: "library store schema is not current",
        });
    }
    Ok(connection)
}

fn require_authoritative(connection: &Connection) -> Result<(), StorageError> {
    let state: Option<String> = connection
        .query_row(
            "SELECT state FROM pod0_domain_cutovers WHERE domain='listening'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read library authority", error))?;
    if state.as_deref() == Some("authoritative") {
        Ok(())
    } else {
        Err(StorageError::CutoverNotAuthoritative)
    }
}
