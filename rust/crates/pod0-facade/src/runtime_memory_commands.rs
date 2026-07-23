use pod0_application::{CommandEnvelope, OperationResult};
use pod0_domain::MemorySource;

use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn create_memory(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        content: &str,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.create_memory(
                    envelope.command_id,
                    fingerprint,
                    content,
                    MemorySource::Agent,
                    self.now().value,
                )
            });
        match result {
            Ok((collection_revision, memory_id, memory_revision)) => self.finish_memory_storage(
                envelope.command_id,
                OperationResult::MemoryCreated {
                    memory_id,
                    memory_revision,
                    collection_revision,
                },
            ),
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    pub(super) fn update_memory(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        memory_id: pod0_domain::MemoryId,
        expected_revision: pod0_domain::MemoryRevision,
        content: &str,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.update_memory(
                    envelope.command_id,
                    fingerprint,
                    memory_id,
                    expected_revision,
                    content,
                    self.now().value,
                )
            });
        match result {
            Ok((collection_revision, memory_revision)) => self.finish_memory_storage(
                envelope.command_id,
                OperationResult::MemoryUpdated {
                    memory_id,
                    memory_revision,
                    collection_revision,
                },
            ),
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    pub(super) fn set_memory_deleted(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        memory_id: pod0_domain::MemoryId,
        expected_revision: pod0_domain::MemoryRevision,
        deleted: bool,
    ) {
        let result = self
            .store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)
            .and_then(|store| {
                store.set_memory_deleted(
                    envelope.command_id,
                    fingerprint,
                    memory_id,
                    expected_revision,
                    deleted,
                    self.now().value,
                )
            });
        match result {
            Ok((collection_revision, memory_revision)) => self.finish_memory_storage(
                envelope.command_id,
                OperationResult::MemoryUpdated {
                    memory_id,
                    memory_revision,
                    collection_revision,
                },
            ),
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    pub(super) fn clear_memories(
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
                store.clear_memories(
                    envelope.command_id,
                    fingerprint,
                    expected_collection_revision,
                    self.now().value,
                )
            });
        match result {
            Ok(collection_revision) => self.finish_memory_storage(
                envelope.command_id,
                OperationResult::MemoriesCleared {
                    collection_revision,
                },
            ),
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    fn finish_memory_storage(
        &mut self,
        command_id: pod0_domain::CommandId,
        result: OperationResult,
    ) {
        match self.reload_memories() {
            Ok(()) => self.succeed(command_id, Some(result)),
            Err(error) => self.fail(command_id, storage_failure(error)),
        }
    }

    pub(super) fn reload_memories(&mut self) -> Result<(), pod0_storage::StorageError> {
        if let Some(store) = &self.store {
            let memories = store.memory_snapshot()?;
            self.revision =
                pod0_domain::StateRevision::new(self.revision.value.max(memories.revision.value));
            self.memories = memories;
        }
        Ok(())
    }
}
