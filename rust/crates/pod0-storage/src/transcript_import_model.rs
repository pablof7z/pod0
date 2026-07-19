use std::path::PathBuf;

use pod0_domain::{
    CommandId, ContentDigest, EpisodeId, PodcastId, StateRevision, TranscriptArtifactId,
    TranscriptVersionId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyTranscriptSourceKind {
    ArtifactSqliteV0,
    ArtifactSqliteV1,
}

impl LegacyTranscriptSourceKind {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::ArtifactSqliteV0 => "artifact_sqlite_v0",
            Self::ArtifactSqliteV1 => "artifact_sqlite_v1",
        }
    }

    pub(crate) const fn schema_version(self) -> u32 {
        match self {
            Self::ArtifactSqliteV0 => 0,
            Self::ArtifactSqliteV1 => 1,
        }
    }

    pub(crate) fn from_code(value: &str) -> Option<Self> {
        match value {
            "artifact_sqlite_v0" => Some(Self::ArtifactSqliteV0),
            "artifact_sqlite_v1" => Some(Self::ArtifactSqliteV1),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptImportState {
    Staged,
    Verified,
    Committed,
    Corrupt,
    Discarded,
}

impl TranscriptImportState {
    pub(crate) fn from_code(value: &str) -> Option<Self> {
        match value {
            "staged" => Some(Self::Staged),
            "verified" => Some(Self::Verified),
            "committed" => Some(Self::Committed),
            "corrupt" => Some(Self::Corrupt),
            "discarded" => Some(Self::Discarded),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptImportPlan {
    pub source_kind: LegacyTranscriptSourceKind,
    pub source_generation: u64,
    pub source_database_digest: ContentDigest,
    pub source_selection_digest: ContentDigest,
    pub selected_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptBackupEvidence {
    pub database_digest: ContentDigest,
    pub database_byte_count: u64,
    pub artifact_count: u32,
    pub artifact_byte_count: u64,
    pub reused_database: bool,
    pub reused_artifacts: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptImportReport {
    pub import_id: CommandId,
    pub plan: TranscriptImportPlan,
    pub target_revision: StateRevision,
    pub backup: TranscriptBackupEvidence,
    pub state: TranscriptImportState,
    pub diagnostic_code: Option<String>,
    pub reused_existing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptImportEntrySummary {
    pub episode_id: EpisodeId,
    pub selected_row_digest: ContentDigest,
    pub selected_file_digest: ContentDigest,
    pub artifact_id: TranscriptArtifactId,
    pub transcript_version_id: TranscriptVersionId,
    pub transcript_content_digest: ContentDigest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptImportVerification {
    pub report: TranscriptImportReport,
    pub verified_artifact_count: u32,
    pub verified_segment_count: u64,
    pub verified_word_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InspectedTranscriptSource {
    pub(crate) plan: TranscriptImportPlan,
    pub(crate) entries: Vec<InspectedTranscriptEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InspectedTranscriptEntry {
    pub(crate) episode_id: EpisodeId,
    pub(crate) podcast_id: PodcastId,
    pub(crate) legacy_row_id: u64,
    pub(crate) legacy_schema_version: u32,
    pub(crate) legacy_input_version: String,
    pub(crate) legacy_output_version: String,
    pub(crate) legacy_origin: Option<String>,
    pub(crate) legacy_integrity: String,
    pub(crate) legacy_verified_at_ms: i64,
    pub(crate) selected_row_digest: ContentDigest,
    pub(crate) selected_file_digest: ContentDigest,
    pub(crate) selected_file_byte_count: u64,
    pub(crate) selected_file_path: PathBuf,
    pub(crate) artifact_id: TranscriptArtifactId,
    pub(crate) transcript_version_id: TranscriptVersionId,
    pub(crate) transcript_content_digest: ContentDigest,
    pub(crate) artifact_integrity_digest: ContentDigest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredTranscriptImportEntry {
    pub(crate) episode_id: EpisodeId,
    pub(crate) legacy_row_id: u64,
    pub(crate) selected_row_digest: ContentDigest,
    pub(crate) selected_file_digest: ContentDigest,
    pub(crate) backup_file_digest: ContentDigest,
    pub(crate) backup_file_byte_count: u64,
    pub(crate) artifact_id: TranscriptArtifactId,
    pub(crate) transcript_version_id: TranscriptVersionId,
}
