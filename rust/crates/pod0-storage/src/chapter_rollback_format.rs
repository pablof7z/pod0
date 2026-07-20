use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct ChapterRollbackManifest {
    pub(crate) format_version: u32,
    pub(crate) core_schema_version: u32,
    pub(crate) source_kind: String,
    pub(crate) source_generation: u64,
    pub(crate) source_database_digest: String,
    pub(crate) source_selection_digest: String,
    pub(crate) evidence_count: u32,
    pub(crate) artifact_count: u32,
    pub(crate) selected_count: u32,
    pub(crate) blocked_count: u32,
    pub(crate) original_database_path: String,
    pub(crate) database_path: String,
    pub(crate) entries: Vec<ChapterRollbackEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct ChapterRollbackEntry {
    pub(crate) evidence_id: String,
    pub(crate) evidence_kind: String,
    pub(crate) source_subject: String,
    pub(crate) source_row_id: Option<u64>,
    pub(crate) raw_digest: String,
    pub(crate) raw_byte_count: u64,
    pub(crate) relative_path: String,
    pub(crate) importer_selected: bool,
    pub(crate) validation_state: String,
    pub(crate) diagnostic_code: Option<String>,
    pub(crate) artifact_id: Option<String>,
}
