use pod0_domain::{
    CompiledMemoryRecord, ContentDigest, MemoryRecord, MemoryRevision, MemorySource,
};
use pod0_storage::{
    LegacyMemoryCutoverInput, StorageError, memory_source_fingerprint, memory_source_generation,
};

use crate::{
    LegacyCompiledMemoryInput, LegacyMemoryCutoverProjection, LegacyMemoryInput, Pod0Facade,
};

#[uniffi::export]
impl Pod0Facade {
    pub fn memory_cutover(&self) -> LegacyMemoryCutoverProjection {
        let state = self.state();
        let Some(store) = state.store.as_ref() else {
            return LegacyMemoryCutoverProjection::blocked(StorageError::CutoverNotAuthoritative);
        };
        store
            .memory_cutover_report()
            .map(LegacyMemoryCutoverProjection::from_report)
            .unwrap_or_else(LegacyMemoryCutoverProjection::blocked)
    }

    pub fn inspect_legacy_memory_cutover(
        &self,
        backup_digest: ContentDigest,
        backup_byte_count: u64,
        memories: Vec<LegacyMemoryInput>,
        compiled: Option<LegacyCompiledMemoryInput>,
    ) -> LegacyMemoryCutoverProjection {
        let observed_at = self.state().now();
        let memory_count = memories.len();
        let deleted_count = memories.iter().filter(|memory| memory.deleted).count();
        let compiled_present = compiled.is_some();
        let input = map_input(
            backup_digest,
            backup_byte_count,
            memories,
            compiled,
            observed_at,
        );
        match input.and_then(|input| {
            let fingerprint = memory_source_fingerprint(&input)?;
            Ok((fingerprint, memory_source_generation(fingerprint)))
        }) {
            Ok((fingerprint, generation)) => LegacyMemoryCutoverProjection::inspected(
                fingerprint,
                generation,
                backup_digest,
                backup_byte_count,
                memory_count,
                deleted_count,
                compiled_present,
            ),
            Err(error) => LegacyMemoryCutoverProjection::blocked(error),
        }
    }

    pub fn stage_legacy_memory_cutover(
        &self,
        backup_digest: ContentDigest,
        backup_byte_count: u64,
        memories: Vec<LegacyMemoryInput>,
        compiled: Option<LegacyCompiledMemoryInput>,
    ) -> LegacyMemoryCutoverProjection {
        let state = self.state();
        let observed_at = state.now();
        let result = map_input(
            backup_digest,
            backup_byte_count,
            memories,
            compiled,
            observed_at,
        )
        .and_then(|input| {
            state
                .store
                .as_ref()
                .ok_or(StorageError::CutoverNotAuthoritative)?
                .stage_legacy_memory_cutover(input)
        });
        result
            .map(LegacyMemoryCutoverProjection::from_report)
            .unwrap_or_else(LegacyMemoryCutoverProjection::blocked)
    }

    pub fn verify_legacy_memory_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyMemoryCutoverProjection {
        let state = self.state();
        let observed_at = state.now().value();
        state
            .store
            .as_ref()
            .ok_or(StorageError::CutoverNotAuthoritative)
            .and_then(|store| store.verify_legacy_memory_cutover(source_generation, observed_at))
            .map(LegacyMemoryCutoverProjection::from_report)
            .unwrap_or_else(LegacyMemoryCutoverProjection::blocked)
    }

    pub fn commit_legacy_memory_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyMemoryCutoverProjection {
        let mut state = self.state();
        let observed_at = state.now().value();
        let result = state
            .store
            .as_ref()
            .ok_or(StorageError::CutoverNotAuthoritative)
            .and_then(|store| store.commit_legacy_memory_cutover(source_generation, observed_at));
        match result {
            Ok(report) => match state.reload_memories() {
                Ok(()) => LegacyMemoryCutoverProjection::from_report(report),
                Err(error) => LegacyMemoryCutoverProjection::blocked(error),
            },
            Err(error) => LegacyMemoryCutoverProjection::blocked(error),
        }
    }

    pub fn discard_staged_legacy_memory_cutover(
        &self,
        source_generation: u64,
    ) -> LegacyMemoryCutoverProjection {
        let state = self.state();
        let Some(store) = state.store.as_ref() else {
            return LegacyMemoryCutoverProjection::blocked(StorageError::CutoverNotAuthoritative);
        };
        match store.discard_staged_legacy_memory_cutover(source_generation) {
            Ok(_) => store
                .memory_cutover_report()
                .map(LegacyMemoryCutoverProjection::from_report)
                .unwrap_or_else(LegacyMemoryCutoverProjection::blocked),
            Err(error) => LegacyMemoryCutoverProjection::blocked(error),
        }
    }
}

fn map_input(
    backup_digest: ContentDigest,
    backup_byte_count: u64,
    memories: Vec<LegacyMemoryInput>,
    compiled: Option<LegacyCompiledMemoryInput>,
    observed_at: pod0_domain::UnixTimestampMilliseconds,
) -> Result<LegacyMemoryCutoverInput, StorageError> {
    let memories = memories
        .into_iter()
        .map(|memory| MemoryRecord {
            memory_id: memory.memory_id,
            revision: MemoryRevision::INITIAL,
            content: memory.content,
            source: MemorySource::LegacySwift,
            created_at: memory.created_at,
            updated_at: memory.created_at,
            deleted: memory.deleted,
        })
        .collect();
    let compiled = compiled.map(|compiled| CompiledMemoryRecord {
        text: compiled.text,
        compiled_at: compiled.compiled_at,
        source_memory_ids: compiled.source_memory_ids,
    });
    Ok(LegacyMemoryCutoverInput {
        backup_digest,
        backup_byte_count,
        memories,
        compiled,
        observed_at,
    })
}
