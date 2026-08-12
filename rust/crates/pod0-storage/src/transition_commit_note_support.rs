use pod0_domain::{EpisodeId, NoteTarget, StateRevision};
use rusqlite::OptionalExtension;

use super::application_support::{next_core_revision, revision};
use crate::StorageError;

pub(super) fn revisions(
    connection: &rusqlite::Connection,
) -> Result<(StateRevision, StateRevision), StorageError> {
    let current: i64 = connection
        .query_row(
            "SELECT collection_revision FROM pod0_note_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read note transition revision", error))?;
    Ok((
        revision(current)?,
        next_core_revision(connection, "read note core revision")?,
    ))
}

pub(super) fn require_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    if crate::library_store_note_support::collection_revision(transaction)? == expected {
        Ok(())
    } else {
        Err(StorageError::RevisionConflict)
    }
}

pub(super) fn episode_for_target(
    connection: &rusqlite::Connection,
    target: Option<NoteTarget>,
) -> Result<Option<EpisodeId>, StorageError> {
    let row = match target {
        None | Some(NoteTarget::Unsupported { .. }) => None,
        Some(NoteTarget::Episode { episode_id, .. }) => return Ok(Some(episode_id)),
        Some(NoteTarget::Note { note_id }) => connection
            .query_row(
                "SELECT episode_id FROM pod0_notes WHERE note_id=?1",
                [note_id.into_bytes().as_slice()],
                |row| row.get::<_, Option<Vec<u8>>>(0),
            )
            .optional()
            .map_err(|error| StorageError::sqlite("read note target episode", error))?
            .flatten(),
        Some(NoteTarget::Clip { clip_id }) => connection
            .query_row(
                "SELECT episode_id FROM pod0_clips WHERE clip_id=?1",
                [clip_id.into_bytes().as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(|error| StorageError::sqlite("read clip target episode", error))?,
    };
    row.map(|bytes| episode_id(&bytes)).transpose()
}

fn episode_id(bytes: &[u8]) -> Result<EpisodeId, StorageError> {
    let value: [u8; 16] = bytes.try_into().map_err(|_| StorageError::CorruptSchema {
        detail: "note episode identity is malformed",
    })?;
    Ok(EpisodeId::from_bytes(value))
}
