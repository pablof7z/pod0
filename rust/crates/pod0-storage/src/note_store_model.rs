use pod0_domain::{NoteRecord, StateRevision};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteCollectionSnapshot {
    pub revision: StateRevision,
    pub notes: Vec<NoteRecord>,
}
