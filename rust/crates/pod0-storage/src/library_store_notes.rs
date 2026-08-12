use pod0_domain::{
    CommandId, NoteId, NoteKind, NoteRevision, NoteTarget, StateRevision, validate_new_note,
};
use rusqlite::params;

use crate::StorageError;
use crate::library_store::{LibraryStore, command_was_applied};
use crate::library_store_note_support::{
    collection_revision, finish_note_command, note_mutation_state, selected_evidence,
    validate_target_reference,
};
use crate::note_store_codec::{encode_kind, encode_target};
use crate::note_store_read::require_notes_authoritative;

impl LibraryStore {
    #[allow(clippy::too_many_arguments)]
    pub fn update_note(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        note_id: NoteId,
        expected_revision: NoteRevision,
        text: &str,
        kind: NoteKind,
        target: Option<NoteTarget>,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        crate::transition_commit::commit_note_update(
            self.path(),
            command_id,
            command_fingerprint,
            note_id,
            expected_revision,
            text,
            kind,
            target,
            observed_at_ms,
        )
    }

    pub fn set_note_deleted(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        note_id: NoteId,
        expected_revision: NoteRevision,
        deleted: bool,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        crate::transition_commit::commit_note_deleted(
            self.path(),
            command_id,
            command_fingerprint,
            note_id,
            expected_revision,
            deleted,
            observed_at_ms,
        )
    }

    pub fn clear_notes(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        expected_collection_revision: StateRevision,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        crate::transition_commit::commit_note_clear(
            self.path(),
            command_id,
            command_fingerprint,
            expected_collection_revision,
            observed_at_ms,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_note_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    command_fingerprint: &str,
    note_id: NoteId,
    expected_revision: NoteRevision,
    text: &str,
    kind: NoteKind,
    target: Option<NoteTarget>,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    require_notes_authoritative(transaction)?;
    if let Some(revision) = command_was_applied(transaction, command_id, command_fingerprint)? {
        return Ok(revision);
    }
    let (stored_revision, author, old_target) = note_mutation_state(transaction, note_id)?;
    if stored_revision != expected_revision.value {
        return Err(StorageError::RevisionConflict);
    }
    validate_new_note(text, kind, author, target).map_err(|_| StorageError::InvalidNote)?;
    validate_target_reference(transaction, note_id, target)?;
    let (kind_code, kind_wire) = encode_kind(kind);
    let encoded_target = encode_target(target)?;
    let target_changed = old_target != target;
    let evidence = if target_changed {
        selected_evidence(transaction, target)?
    } else {
        None
    };
    let changed = transaction.execute(
        "UPDATE pod0_notes SET text=?1,kind_code=?2,kind_wire_code=?3,target_code=?4,\
         target_wire_code=?5,target_note_id=?6,episode_id=?7,position_ms=?8,target_clip_id=?9,\
         note_revision=note_revision+1,evidence_generation_id=CASE WHEN ?10 THEN ?11 ELSE \
         evidence_generation_id END,evidence_transcript_version_id=CASE WHEN ?10 THEN ?12 ELSE \
         evidence_transcript_version_id END,evidence_content_digest=CASE WHEN ?10 THEN ?13 ELSE \
         evidence_content_digest END,evidence_span_id=CASE WHEN ?10 THEN ?14 ELSE evidence_span_id END \
         WHERE note_id=?15 AND note_revision=?16",
        params![text, kind_code, kind_wire, encoded_target.code, encoded_target.wire,
            encoded_target.note_id, encoded_target.episode_id, encoded_target.position_ms,
            encoded_target.clip_id, i64::from(target_changed),
            evidence.map(|value| value.generation_id.into_bytes().to_vec()),
            evidence.map(|value| value.transcript_version_id.into_bytes().to_vec()),
            evidence.map(|value| value.transcript_content_digest.into_bytes().to_vec()),
            evidence.map(|value| value.span_id.into_bytes().to_vec()),
            note_id.into_bytes().as_slice(), i64::try_from(expected_revision.value)
                .map_err(|_| StorageError::RevisionConflict)?],
    ).map_err(|error| StorageError::sqlite("update note", error))?;
    if changed != 1 {
        return Err(StorageError::RevisionConflict);
    }
    finish_note_command(transaction, command_id, command_fingerprint, observed_at_ms)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn set_note_deleted_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    command_fingerprint: &str,
    note_id: NoteId,
    expected_revision: NoteRevision,
    deleted: bool,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    require_notes_authoritative(transaction)?;
    if let Some(revision) = command_was_applied(transaction, command_id, command_fingerprint)? {
        return Ok(revision);
    }
    let (stored_revision, _, _) = note_mutation_state(transaction, note_id)?;
    if stored_revision != expected_revision.value {
        return Err(StorageError::RevisionConflict);
    }
    let changed = transaction
        .execute(
            "UPDATE pod0_notes SET deleted=?1,note_revision=note_revision+1 \
         WHERE note_id=?2 AND note_revision=?3",
            params![
                i64::from(deleted),
                note_id.into_bytes().as_slice(),
                i64::try_from(expected_revision.value)
                    .map_err(|_| StorageError::RevisionConflict)?
            ],
        )
        .map_err(|error| StorageError::sqlite("update note deletion", error))?;
    if changed != 1 {
        return Err(StorageError::RevisionConflict);
    }
    finish_note_command(transaction, command_id, command_fingerprint, observed_at_ms)
}

pub(crate) fn clear_notes_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    command_fingerprint: &str,
    expected_collection_revision: StateRevision,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    require_notes_authoritative(transaction)?;
    if let Some(revision) = command_was_applied(transaction, command_id, command_fingerprint)? {
        return Ok(revision);
    }
    if collection_revision(transaction)? != expected_collection_revision {
        return Err(StorageError::RevisionConflict);
    }
    transaction
        .execute(
            "UPDATE pod0_notes SET deleted=1,note_revision=note_revision+1 WHERE deleted=0",
            [],
        )
        .map_err(|error| StorageError::sqlite("clear notes", error))?;
    finish_note_command(transaction, command_id, command_fingerprint, observed_at_ms)
}
