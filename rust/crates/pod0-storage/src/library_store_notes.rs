use pod0_domain::{
    CommandId, NoteAuthor, NoteId, NoteKind, NoteRevision, NoteTarget, StateRevision,
    validate_new_note,
};
use rusqlite::params;

use crate::StorageError;
use crate::library_store::{LibraryStore, command_was_applied};
use crate::library_store_note_support::{
    collection_revision, finish_note_command, note_exists, note_mutation_state, require_note,
    selected_evidence, validate_target_reference,
};
use crate::note_store_codec::{encode_author, encode_kind, encode_target};
use crate::note_store_read::require_notes_authoritative;

impl LibraryStore {
    #[allow(clippy::too_many_arguments)]
    pub fn create_note(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        text: &str,
        kind: NoteKind,
        author: NoteAuthor,
        target: Option<NoteTarget>,
        observed_at_ms: i64,
    ) -> Result<(StateRevision, NoteId), StorageError> {
        self.write(|transaction| {
            require_notes_authoritative(transaction)?;
            let note_id = NoteId::from_bytes(command_id.into_bytes());
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
                require_note(transaction, note_id)?;
                return Ok((revision, note_id));
            }
            validate_new_note(text, kind, author, target)
                .map_err(|_| StorageError::InvalidNote)?;
            validate_target_reference(transaction, note_id, target)?;
            if note_exists(transaction, note_id)? {
                return Err(StorageError::CommandConflict);
            }
            let (kind_code, kind_wire) = encode_kind(kind);
            let (author_code, author_wire) = encode_author(author);
            let encoded_target = encode_target(target)?;
            let evidence = selected_evidence(transaction, target)?;
            let evidence_generation = evidence.map(|value| value.generation_id.into_bytes().to_vec());
            let evidence_version = evidence.map(|value| value.transcript_version_id.into_bytes().to_vec());
            let evidence_digest = evidence.map(|value| value.transcript_content_digest.into_bytes().to_vec());
            let evidence_span = evidence.map(|value| value.span_id.into_bytes().to_vec());
            transaction.execute(
                "INSERT INTO pod0_notes(note_id,note_revision,text,kind_code,kind_wire_code,\
                 author_code,author_wire_code,target_code,target_wire_code,target_note_id,episode_id,\
                 position_ms,created_at_ms,deleted,evidence_generation_id,\
                 evidence_transcript_version_id,evidence_content_digest,evidence_span_id,\
                 source_import_id,created_command_id) \
                 VALUES(?1,1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,0,?13,?14,?15,?16,NULL,?17)",
                params![
                    note_id.into_bytes().as_slice(),
                    text,
                    kind_code,
                    kind_wire,
                    author_code,
                    author_wire,
                    encoded_target.code,
                    encoded_target.wire,
                    encoded_target.note_id,
                    encoded_target.episode_id,
                    encoded_target.position_ms,
                    observed_at_ms,
                    evidence_generation,
                    evidence_version,
                    evidence_digest,
                    evidence_span,
                    command_id.into_bytes().as_slice(),
                ],
            ).map_err(|error| StorageError::sqlite("create note", error))?;
            let revision = finish_note_command(
                transaction,
                command_id,
                command_fingerprint,
                observed_at_ms,
            )?;
            Ok((revision, note_id))
        })
    }

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
        self.write(|transaction| {
            require_notes_authoritative(transaction)?;
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
                return Ok(revision);
            }
            let (stored_revision, author, old_target) = note_mutation_state(transaction, note_id)?;
            if stored_revision != expected_revision.value {
                return Err(StorageError::RevisionConflict);
            }
            validate_new_note(text, kind, author, target)
                .map_err(|_| StorageError::InvalidNote)?;
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
                 target_wire_code=?5,target_note_id=?6,episode_id=?7,position_ms=?8,\
                 note_revision=note_revision+1,\
                 evidence_generation_id=CASE WHEN ?9 THEN ?10 ELSE evidence_generation_id END,\
                 evidence_transcript_version_id=CASE WHEN ?9 THEN ?11 ELSE evidence_transcript_version_id END,\
                 evidence_content_digest=CASE WHEN ?9 THEN ?12 ELSE evidence_content_digest END,\
                 evidence_span_id=CASE WHEN ?9 THEN ?13 ELSE evidence_span_id END \
                 WHERE note_id=?14 AND note_revision=?15",
                params![
                    text,
                    kind_code,
                    kind_wire,
                    encoded_target.code,
                    encoded_target.wire,
                    encoded_target.note_id,
                    encoded_target.episode_id,
                    encoded_target.position_ms,
                    i64::from(target_changed),
                    evidence.map(|value| value.generation_id.into_bytes().to_vec()),
                    evidence.map(|value| value.transcript_version_id.into_bytes().to_vec()),
                    evidence.map(|value| value.transcript_content_digest.into_bytes().to_vec()),
                    evidence.map(|value| value.span_id.into_bytes().to_vec()),
                    note_id.into_bytes().as_slice(),
                    i64::try_from(expected_revision.value).map_err(|_| StorageError::RevisionConflict)?,
                ],
            ).map_err(|error| StorageError::sqlite("update note", error))?;
            if changed != 1 {
                return Err(StorageError::RevisionConflict);
            }
            finish_note_command(transaction, command_id, command_fingerprint, observed_at_ms)
        })
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
        self.write(|transaction| {
            require_notes_authoritative(transaction)?;
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
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
                            .map_err(|_| StorageError::RevisionConflict)?,
                    ],
                )
                .map_err(|error| StorageError::sqlite("update note deletion", error))?;
            if changed != 1 {
                return Err(StorageError::RevisionConflict);
            }
            finish_note_command(transaction, command_id, command_fingerprint, observed_at_ms)
        })
    }

    pub fn clear_notes(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        expected_collection_revision: StateRevision,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        self.write(|transaction| {
            require_notes_authoritative(transaction)?;
            if let Some(revision) =
                command_was_applied(transaction, command_id, command_fingerprint)?
            {
                return Ok(revision);
            }
            let current = collection_revision(transaction)?;
            if current != expected_collection_revision {
                return Err(StorageError::RevisionConflict);
            }
            transaction
                .execute(
                    "UPDATE pod0_notes SET deleted=1,note_revision=note_revision+1 WHERE deleted=0",
                    [],
                )
                .map_err(|error| StorageError::sqlite("clear notes", error))?;
            finish_note_command(transaction, command_id, command_fingerprint, observed_at_ms)
        })
    }
}
