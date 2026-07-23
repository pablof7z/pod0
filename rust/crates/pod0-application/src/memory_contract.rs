use pod0_domain::{CompiledMemoryRecord, MemoryRecord, StateRevision};

use crate::{MAX_OPERATION_ITEMS, MAX_PROJECTION_ITEMS, OperationProjection};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MemoryProjectionScope {
    All,
    Active,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MemoriesProjection {
    pub scope: MemoryProjectionScope,
    pub collection_revision: StateRevision,
    pub memories: Vec<MemoryRecord>,
    pub compiled: Option<CompiledMemoryRecord>,
    pub operations: Vec<OperationProjection>,
    pub has_more: bool,
}

impl MemoriesProjection {
    pub fn enforce_bounds(&mut self, offset: usize, requested_items: usize) {
        let limit = requested_items.clamp(1, usize::from(MAX_PROJECTION_ITEMS));
        let count = self.memories.len();
        self.memories = std::mem::take(&mut self.memories)
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();
        self.operations.truncate(MAX_OPERATION_ITEMS);
        self.has_more |= count > offset.saturating_add(self.memories.len());
    }
}
