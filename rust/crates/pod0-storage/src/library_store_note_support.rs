use pod0_domain::{
    CommandId, NoteAuthor, NoteEvidenceReference, NoteId, NoteTarget, StateRevision,
};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::library_store::finish_command;
use crate::{StorageError, note_store_codec};

pub(crate) fn finish_note_command(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    fingerprint: &str,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let revision = finish_command(transaction, command_id, fingerprint, observed_at_ms)?;
    let value = i64::try_from(revision.value).map_err(|_| StorageError::CorruptSchema {
        detail: "note collection revision is malformed",
    })?;
    transaction
        .execute(
            "UPDATE pod0_note_state SET collection_revision=?1 WHERE singleton=1",
            [value],
        )
        .map_err(|error| StorageError::sqlite("advance note collection revision", error))?;
    transaction
        .execute(
            "UPDATE pod0_domain_cutovers SET core_revision=?1 WHERE domain='notes'",
            [value],
        )
        .map_err(|error| StorageError::sqlite("advance note cutover revision", error))?;
    Ok(revision)
}

pub(crate) fn collection_revision(
    transaction: &Transaction<'_>,
) -> Result<StateRevision, StorageError> {
    let value: i64 = transaction
        .query_row(
            "SELECT collection_revision FROM pod0_note_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read note collection revision", error))?;
    Ok(StateRevision::new(u64::try_from(value).map_err(|_| {
        StorageError::CorruptSchema {
            detail: "note collection revision is malformed",
        }
    })?))
}

pub(crate) fn note_exists(
    transaction: &Transaction<'_>,
    note_id: NoteId,
) -> Result<bool, StorageError> {
    transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pod0_notes WHERE note_id=?1)",
            [note_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("find note", error))
}

pub(crate) fn require_note(
    transaction: &Transaction<'_>,
    note_id: NoteId,
) -> Result<(), StorageError> {
    if note_exists(transaction, note_id)? {
        Ok(())
    } else {
        Err(StorageError::EntityNotFound)
    }
}

pub(crate) fn note_mutation_state(
    transaction: &Transaction<'_>,
    note_id: NoteId,
) -> Result<(u64, NoteAuthor, Option<NoteTarget>), StorageError> {
    let row = transaction
        .query_row(
            "SELECT note_revision,author_code,author_wire_code,target_code,target_wire_code,\
             target_note_id,episode_id,position_ms FROM pod0_notes WHERE note_id=?1",
            [note_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                    row.get::<_, Option<Vec<u8>>>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read note mutation state", error))?
        .ok_or(StorageError::EntityNotFound)?;
    Ok((
        u64::try_from(row.0).map_err(|_| StorageError::CorruptSchema {
            detail: "note revision is malformed",
        })?,
        note_store_codec::decode_author(row.1, row.2)?,
        note_store_codec::decode_target(row.3, row.4, row.5, row.6, row.7)?,
    ))
}

pub(crate) fn selected_evidence(
    transaction: &Transaction<'_>,
    target: Option<NoteTarget>,
) -> Result<Option<NoteEvidenceReference>, StorageError> {
    let Some(NoteTarget::Episode {
        episode_id,
        position_milliseconds,
    }) = target
    else {
        return Ok(None);
    };
    let position = i64::try_from(position_milliseconds).map_err(|_| StorageError::InvalidNote)?;
    let row = transaction
        .query_row(
            "SELECT s.generation_id,g.transcript_version_id,d.content_digest,s.span_id \
             FROM pod0_evidence_selection selected \
             JOIN pod0_evidence_generations g ON g.generation_id=selected.generation_id \
             JOIN pod0_transcript_documents d ON d.transcript_version_id=g.transcript_version_id \
             JOIN pod0_evidence_spans s ON s.generation_id=g.generation_id \
             WHERE selected.episode_id=?1 AND g.state='verified' \
             AND s.start_ms<=?2 AND s.end_ms>?2 ORDER BY s.sort_order LIMIT 1",
            params![episode_id.into_bytes().as_slice(), position],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("select note evidence", error))?;
    let Some((generation, version, digest, span)) = row else {
        return Ok(None);
    };
    note_store_codec::decode_evidence(Some(generation), Some(version), Some(digest), Some(span))
}

pub(crate) fn validate_target_reference(
    transaction: &Transaction<'_>,
    subject_note_id: NoteId,
    target: Option<NoteTarget>,
) -> Result<(), StorageError> {
    let exists = match target {
        None => true,
        Some(NoteTarget::Note { note_id }) if note_id == subject_note_id => false,
        Some(NoteTarget::Note { note_id }) => note_exists(transaction, note_id)?,
        Some(NoteTarget::Episode { episode_id, .. }) => transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pod0_episodes WHERE episode_id=?1)",
                [episode_id.into_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(|error| StorageError::sqlite("validate note episode target", error))?,
        Some(NoteTarget::Unsupported { .. }) => false,
    };
    if exists {
        Ok(())
    } else {
        Err(StorageError::InvalidNote)
    }
}
