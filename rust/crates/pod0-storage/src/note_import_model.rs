use pod0_domain::{CommandId, NoteRecord, StateRevision};

use crate::LegacySourceKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteImportPlan {
    pub source_kind: LegacySourceKind,
    pub source_hash: String,
    pub source_generation: u64,
    pub note_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteBackupEvidence {
    pub source_kind: LegacySourceKind,
    pub source_hash: String,
    pub source_generation: u64,
    pub byte_count: u64,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteImportReport {
    pub import_id: CommandId,
    pub plan: NoteImportPlan,
    pub target_revision: StateRevision,
    pub backup: NoteBackupEvidence,
    pub staged: bool,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteImportVerification {
    pub report: NoteImportReport,
    pub snapshot: crate::NoteCollectionSnapshot,
}

pub(crate) struct InspectedNoteSource {
    pub(crate) plan: NoteImportPlan,
    pub(crate) notes: Vec<NoteRecord>,
}
