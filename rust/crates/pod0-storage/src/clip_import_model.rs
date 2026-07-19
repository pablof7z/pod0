use pod0_domain::{ClipRecord, CommandId, StateRevision};

use crate::LegacySourceKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipImportPlan {
    pub source_kind: LegacySourceKind,
    pub source_hash: String,
    pub source_generation: u64,
    pub clip_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipBackupEvidence {
    pub source_kind: LegacySourceKind,
    pub source_hash: String,
    pub source_generation: u64,
    pub byte_count: u64,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipImportReport {
    pub import_id: CommandId,
    pub plan: ClipImportPlan,
    pub target_revision: StateRevision,
    pub backup: ClipBackupEvidence,
    pub staged: bool,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipImportVerification {
    pub report: ClipImportReport,
    pub snapshot: crate::ClipCollectionSnapshot,
}

pub(crate) struct InspectedClipSource {
    pub(crate) plan: ClipImportPlan,
    pub(crate) clips: Vec<ClipRecord>,
}
