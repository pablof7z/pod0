use pod0_domain::{CommandId, MemoryId, MemoryRevision, MemorySource, StateRevision};

use crate::StorageError;
use crate::library_store::LibraryStore;

impl LibraryStore {
    pub fn create_memory(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        content: &str,
        source: MemorySource,
        observed_at_ms: i64,
    ) -> Result<(StateRevision, MemoryId, MemoryRevision), StorageError> {
        crate::transition_commit::commit_memory_create(
            self.path(),
            command_id,
            command_fingerprint,
            content,
            source,
            observed_at_ms,
        )
    }

    pub fn update_memory(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        memory_id: MemoryId,
        expected_revision: MemoryRevision,
        content: &str,
        observed_at_ms: i64,
    ) -> Result<(StateRevision, MemoryRevision), StorageError> {
        crate::transition_commit::commit_memory_update(
            self.path(),
            command_id,
            command_fingerprint,
            memory_id,
            expected_revision,
            content,
            observed_at_ms,
        )
    }

    pub fn set_memory_deleted(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        memory_id: MemoryId,
        expected_revision: MemoryRevision,
        deleted: bool,
        observed_at_ms: i64,
    ) -> Result<(StateRevision, MemoryRevision), StorageError> {
        crate::transition_commit::commit_memory_deleted(
            self.path(),
            command_id,
            command_fingerprint,
            memory_id,
            expected_revision,
            deleted,
            observed_at_ms,
        )
    }

    pub fn clear_memories(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        expected_collection_revision: StateRevision,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        crate::transition_commit::commit_memory_clear(
            self.path(),
            command_id,
            command_fingerprint,
            expected_collection_revision,
            observed_at_ms,
        )
    }
}
