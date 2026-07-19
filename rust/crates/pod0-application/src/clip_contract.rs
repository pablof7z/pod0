use pod0_domain::{ClipId, ClipRecord, EpisodeId, StateRevision};

use crate::{MAX_OPERATION_ITEMS, MAX_PROJECTION_ITEMS, OperationProjection};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ClipProjectionScope {
    All,
    Active,
    Clip { clip_id: ClipId },
    Episode { episode_id: EpisodeId },
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ClipsProjection {
    pub scope: ClipProjectionScope,
    pub collection_revision: StateRevision,
    pub clips: Vec<ClipRecord>,
    pub operations: Vec<OperationProjection>,
    pub has_more: bool,
}

impl ClipsProjection {
    pub fn enforce_bounds(&mut self, offset: usize, requested_items: usize) {
        let limit = requested_items.clamp(1, usize::from(MAX_PROJECTION_ITEMS));
        let count = self.clips.len();
        self.clips = std::mem::take(&mut self.clips)
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();
        self.operations.truncate(MAX_OPERATION_ITEMS);
        self.has_more |= count > offset.saturating_add(self.clips.len());
    }
}
