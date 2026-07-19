use pod0_domain::{EpisodeId, NoteRecord, StateRevision};

use crate::{MAX_OPERATION_ITEMS, MAX_PROJECTION_ITEMS, OperationProjection};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum NoteProjectionScope {
    All,
    Active,
    Episode { episode_id: EpisodeId },
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct NotesProjection {
    pub scope: NoteProjectionScope,
    pub collection_revision: StateRevision,
    pub notes: Vec<NoteRecord>,
    pub operations: Vec<OperationProjection>,
    pub has_more: bool,
}

impl NotesProjection {
    pub fn enforce_bounds(&mut self, offset: usize, requested_items: usize) {
        let limit = requested_items.clamp(1, usize::from(MAX_PROJECTION_ITEMS));
        let count = self.notes.len();
        self.notes = std::mem::take(&mut self.notes)
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();
        self.operations.truncate(MAX_OPERATION_ITEMS);
        self.has_more |= count > offset.saturating_add(self.notes.len());
    }
}
