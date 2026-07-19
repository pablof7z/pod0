use pod0_domain::{ClipRecord, StateRevision};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipCollectionSnapshot {
    pub revision: StateRevision,
    pub clips: Vec<ClipRecord>,
}
