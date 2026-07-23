use pod0_domain::{
    CompiledMemoryRecord, ContentDigest, MemoryId, MemoryRecord, UnixTimestampMilliseconds,
};
use sha2::{Digest as _, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryCutoverState {
    NotStarted,
    Staged { source_generation: u64 },
    Verified { source_generation: u64 },
    Authoritative { source_generation: u64 },
}

impl MemoryCutoverState {
    pub const fn source_generation(self) -> Option<u64> {
        match self {
            Self::NotStarted => None,
            Self::Staged { source_generation }
            | Self::Verified { source_generation }
            | Self::Authoritative { source_generation } => Some(source_generation),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyMemoryCutoverInput {
    pub backup_digest: ContentDigest,
    pub backup_byte_count: u64,
    pub memories: Vec<MemoryRecord>,
    pub compiled: Option<CompiledMemoryRecord>,
    pub observed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyMemoryCutoverReport {
    pub state: MemoryCutoverState,
    pub source_fingerprint: Option<ContentDigest>,
    pub backup_digest: Option<ContentDigest>,
    pub backup_byte_count: Option<u64>,
    pub memory_count: u32,
    pub deleted_count: u32,
    pub compiled_present: bool,
}

pub fn memory_source_fingerprint(
    input: &LegacyMemoryCutoverInput,
) -> Result<ContentDigest, crate::StorageError> {
    validate_memory_cutover_input(input)?;
    let mut hash = Sha256::new();
    hash.update(b"pod0-legacy-agent-memory-cutover-v1");
    hash.update(input.backup_digest.into_bytes());
    hash.update(input.backup_byte_count.to_be_bytes());
    let mut memories: Vec<_> = input.memories.iter().collect();
    memories.sort_by_key(|memory| memory.memory_id.into_bytes());
    hash.update((memories.len() as u64).to_be_bytes());
    for memory in memories {
        hash.update(memory.memory_id.into_bytes());
        hash.update((memory.content.len() as u64).to_be_bytes());
        hash.update(memory.content.as_bytes());
        hash.update(memory.created_at.value().to_be_bytes());
        hash.update([u8::from(memory.deleted)]);
    }
    match &input.compiled {
        Some(compiled) => {
            hash.update([1]);
            hash.update((compiled.text.len() as u64).to_be_bytes());
            hash.update(compiled.text.as_bytes());
            hash.update(compiled.compiled_at.value().to_be_bytes());
            hash.update((compiled.source_memory_ids.len() as u64).to_be_bytes());
            for id in &compiled.source_memory_ids {
                hash.update(id.into_bytes());
            }
        }
        None => hash.update([0]),
    }
    Ok(ContentDigest::from_bytes(hash.finalize().into()))
}

#[must_use]
pub fn memory_source_generation(fingerprint: ContentDigest) -> u64 {
    let bytes = fingerprint.into_bytes();
    u64::from_be_bytes(bytes[..8].try_into().expect("digest prefix")) & i64::MAX as u64 | 1
}

pub fn validate_memory_cutover_input(
    input: &LegacyMemoryCutoverInput,
) -> Result<(), crate::StorageError> {
    use std::collections::BTreeSet;

    if input.backup_byte_count == 0 || input.memories.len() > 10_000 {
        return Err(crate::StorageError::InvalidMemory);
    }
    let mut ids = BTreeSet::<MemoryId>::new();
    for memory in &input.memories {
        if !ids.insert(memory.memory_id)
            || memory.revision.value != 1
            || memory.created_at.value() < 0
            || memory.updated_at != memory.created_at
            || !matches!(memory.source, pod0_domain::MemorySource::LegacySwift)
            || pod0_domain::validate_new_memory(&memory.content, memory.source).is_err()
        {
            return Err(crate::StorageError::InvalidMemory);
        }
    }
    if let Some(compiled) = &input.compiled {
        pod0_domain::validate_compiled_memory(compiled)
            .map_err(|_| crate::StorageError::InvalidMemory)?;
        if compiled.compiled_at.value() < 0 {
            return Err(crate::StorageError::InvalidMemory);
        }
        let mut source_ids = BTreeSet::new();
        if compiled
            .source_memory_ids
            .iter()
            .any(|id| !ids.contains(id) || !source_ids.insert(*id))
        {
            return Err(crate::StorageError::InvalidMemory);
        }
    }
    Ok(())
}
