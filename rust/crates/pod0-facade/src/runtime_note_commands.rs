use pod0_application::{CommandEnvelope, OperationResult};

use crate::runtime_commands::storage_failure;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn create_note(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        text: &str,
        kind: pod0_domain::NoteKind,
        author: pod0_domain::NoteAuthor,
        target: Option<pod0_domain::NoteTarget>,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.create_note(
                    envelope.command_id,
                    fingerprint,
                    text,
                    kind,
                    author,
                    target,
                    self.now().value,
                )
            });
        match result {
            Ok((_, note_id)) => self.finish_note_storage(
                envelope.command_id,
                OperationResult::NoteCreated { note_id },
            ),
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn update_note(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        note_id: pod0_domain::NoteId,
        expected_revision: pod0_domain::NoteRevision,
        text: &str,
        kind: pod0_domain::NoteKind,
        target: Option<pod0_domain::NoteTarget>,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.update_note(
                    envelope.command_id,
                    fingerprint,
                    note_id,
                    expected_revision,
                    text,
                    kind,
                    target,
                    self.now().value,
                )
            });
        match result {
            Ok(_) => self.finish_note_storage(
                envelope.command_id,
                OperationResult::NoteUpdated { note_id },
            ),
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    pub(super) fn set_note_deleted(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        note_id: pod0_domain::NoteId,
        expected_revision: pod0_domain::NoteRevision,
        deleted: bool,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.set_note_deleted(
                    envelope.command_id,
                    fingerprint,
                    note_id,
                    expected_revision,
                    deleted,
                    self.now().value,
                )
            });
        match result {
            Ok(_) => self.finish_note_storage(
                envelope.command_id,
                OperationResult::NoteUpdated { note_id },
            ),
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    pub(super) fn clear_notes(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        expected_collection_revision: pod0_domain::StateRevision,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.clear_notes(
                    envelope.command_id,
                    fingerprint,
                    expected_collection_revision,
                    self.now().value,
                )
            });
        match result {
            Ok(_) => self.finish_note_storage(envelope.command_id, OperationResult::NotesCleared),
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    fn finish_note_storage(&mut self, command_id: pod0_domain::CommandId, result: OperationResult) {
        match self.reload_notes() {
            Ok(()) => self.succeed(command_id, Some(result)),
            Err(error) => self.fail(command_id, storage_failure(error)),
        }
    }

    pub(super) fn reload_notes(&mut self) -> Result<(), pod0_storage::StorageError> {
        if let Some(store) = &self.store {
            let notes = store.note_snapshot()?;
            self.revision =
                pod0_domain::StateRevision::new(self.revision.value.max(notes.revision.value));
            self.notes = notes;
        }
        Ok(())
    }
}
