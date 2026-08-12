use pod0_domain::{
    CommandId, NoteAuthor, NoteId, NoteKind, NoteTarget, StateRevision, validate_new_note,
};
use rusqlite::params;

use crate::StorageError;
use crate::library_store::{LibraryStore, command_was_applied};
use crate::library_store_note_support::{
    finish_note_command, note_exists, require_note, selected_evidence, validate_target_reference,
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
        crate::transition_commit::commit_note_create(
            self.path(),
            command_id,
            command_fingerprint,
            text,
            kind,
            author,
            target,
            observed_at_ms,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn create_note_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    command_id: CommandId,
    note_id: NoteId,
    command_fingerprint: &str,
    text: &str,
    kind: NoteKind,
    author: NoteAuthor,
    target: Option<NoteTarget>,
    observed_at_ms: i64,
) -> Result<(StateRevision, NoteId), StorageError> {
    require_notes_authoritative(transaction)?;
    if let Some(revision) = command_was_applied(transaction, command_id, command_fingerprint)? {
        require_note(transaction, note_id)?;
        return Ok((revision, note_id));
    }
    validate_new_note(text, kind, author, target).map_err(|_| StorageError::InvalidNote)?;
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
    let evidence_digest =
        evidence.map(|value| value.transcript_content_digest.into_bytes().to_vec());
    let evidence_span = evidence.map(|value| value.span_id.into_bytes().to_vec());
    transaction
        .execute(
            "INSERT INTO pod0_notes(note_id,note_revision,text,kind_code,kind_wire_code,
         author_code,author_wire_code,target_code,target_wire_code,target_note_id,episode_id,
         position_ms,target_clip_id,created_at_ms,deleted,evidence_generation_id,
         evidence_transcript_version_id,evidence_content_digest,evidence_span_id,
         source_import_id,created_command_id)
         VALUES(?1,1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,0,?14,?15,?16,?17,NULL,?18)",
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
                encoded_target.clip_id,
                observed_at_ms,
                evidence_generation,
                evidence_version,
                evidence_digest,
                evidence_span,
                command_id.into_bytes().as_slice(),
            ],
        )
        .map_err(|error| StorageError::sqlite("create note", error))?;
    let revision =
        finish_note_command(transaction, command_id, command_fingerprint, observed_at_ms)?;
    Ok((revision, note_id))
}
