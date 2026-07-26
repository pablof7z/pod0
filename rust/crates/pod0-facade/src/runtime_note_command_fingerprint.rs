use pod0_application::ApplicationCommand;
use sha2::{Digest, Sha256};

use crate::runtime_command_fingerprint_values::{
    hash_note_author, hash_note_kind, hash_note_target,
};

/// Note and memory arms of the command fingerprint, split out of
/// `runtime_command_fingerprint.rs`.
pub(super) fn hash_note_command(hash: &mut Sha256, command: &ApplicationCommand) {
    match command {
        ApplicationCommand::CreateNote {
            text,
            kind,
            author,
            target,
        } => {
            hash.update(b"create-note\0");
            hash.update(text.as_bytes());
            hash.update([0]);
            hash_note_kind(hash, *kind);
            hash_note_author(hash, *author);
            hash_note_target(hash, *target);
        }
        ApplicationCommand::UpdateNote {
            note_id,
            expected_note_revision,
            text,
            kind,
            target,
        } => {
            hash.update(b"update-note\0");
            hash.update(note_id.into_bytes());
            hash.update(expected_note_revision.value.to_be_bytes());
            hash.update(text.as_bytes());
            hash.update([0]);
            hash_note_kind(hash, *kind);
            hash_note_target(hash, *target);
        }
        ApplicationCommand::SetNoteDeleted {
            note_id,
            expected_note_revision,
            deleted,
        } => {
            hash.update(b"delete-note\0");
            hash.update(note_id.into_bytes());
            hash.update(expected_note_revision.value.to_be_bytes());
            hash.update([u8::from(*deleted)]);
        }
        ApplicationCommand::ClearNotes {
            expected_collection_revision,
        } => {
            hash.update(b"clear-notes\0");
            hash.update(expected_collection_revision.value.to_be_bytes());
        }
        ApplicationCommand::CreateMemory { content } => {
            hash.update(b"create-memory\0");
            hash.update(content.as_bytes());
            hash.update([0]);
        }
        ApplicationCommand::UpdateMemory {
            memory_id,
            expected_memory_revision,
            content,
        } => {
            hash.update(b"update-memory\0");
            hash.update(memory_id.into_bytes());
            hash.update(expected_memory_revision.value.to_be_bytes());
            hash.update(content.as_bytes());
            hash.update([0]);
        }
        ApplicationCommand::SetMemoryDeleted {
            memory_id,
            expected_memory_revision,
            deleted,
        } => {
            hash.update(b"delete-memory\0");
            hash.update(memory_id.into_bytes());
            hash.update(expected_memory_revision.value.to_be_bytes());
            hash.update([u8::from(*deleted)]);
        }
        ApplicationCommand::ClearMemories {
            expected_collection_revision,
        } => {
            hash.update(b"clear-memories\0");
            hash.update(expected_collection_revision.value.to_be_bytes());
        }
        _ => unreachable!("only note and memory commands are routed here"),
    }
}
