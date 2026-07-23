use crate::{MemoryId, UnixTimestampMilliseconds};

pub const MAX_MEMORY_CONTENT_BYTES: usize = 65_536;
pub const MAX_COMPILED_MEMORY_BYTES: usize = 65_536;
pub const MAX_COMPILED_MEMORY_SOURCES: usize = 10_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, uniffi::Record)]
pub struct MemoryRevision {
    pub value: u64,
}

impl MemoryRevision {
    pub const INITIAL: Self = Self { value: 1 };

    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self { value }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MemorySource {
    Agent,
    LegacySwift,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MemoryRecord {
    pub memory_id: MemoryId,
    pub revision: MemoryRevision,
    pub content: String,
    pub source: MemorySource,
    pub created_at: UnixTimestampMilliseconds,
    pub updated_at: UnixTimestampMilliseconds,
    pub deleted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CompiledMemoryRecord {
    pub text: String,
    pub compiled_at: UnixTimestampMilliseconds,
    pub source_memory_ids: Vec<MemoryId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryValidationError {
    EmptyContent,
    ContentTooLarge,
    UnsupportedSource,
    InvalidRevision,
    InvalidTimestamp,
    CompiledTextTooLarge,
    TooManyCompiledSources,
}

pub fn validate_new_memory(
    content: &str,
    source: MemorySource,
) -> Result<(), MemoryValidationError> {
    if content.trim().is_empty() {
        return Err(MemoryValidationError::EmptyContent);
    }
    if content.len() > MAX_MEMORY_CONTENT_BYTES {
        return Err(MemoryValidationError::ContentTooLarge);
    }
    if matches!(source, MemorySource::Unsupported { .. }) {
        return Err(MemoryValidationError::UnsupportedSource);
    }
    Ok(())
}

pub fn validate_compiled_memory(
    compiled: &CompiledMemoryRecord,
) -> Result<(), MemoryValidationError> {
    if compiled.text.len() > MAX_COMPILED_MEMORY_BYTES {
        return Err(MemoryValidationError::CompiledTextTooLarge);
    }
    if compiled.source_memory_ids.len() > MAX_COMPILED_MEMORY_SOURCES {
        return Err(MemoryValidationError::TooManyCompiledSources);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memories_reject_empty_unbounded_and_unsupported_content() {
        assert_eq!(
            validate_new_memory("  ", MemorySource::Agent),
            Err(MemoryValidationError::EmptyContent)
        );
        assert_eq!(
            validate_new_memory(
                &"x".repeat(MAX_MEMORY_CONTENT_BYTES + 1),
                MemorySource::Agent,
            ),
            Err(MemoryValidationError::ContentTooLarge)
        );
        assert!(validate_new_memory("Prefers concise answers", MemorySource::Agent).is_ok());
    }
}
