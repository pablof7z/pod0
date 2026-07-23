use pod0_domain::{CompiledMemoryRecord, MemoryRecord, StateRevision};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryCollectionSnapshot {
    pub revision: StateRevision,
    pub memories: Vec<MemoryRecord>,
    pub compiled: Option<CompiledMemoryRecord>,
}
