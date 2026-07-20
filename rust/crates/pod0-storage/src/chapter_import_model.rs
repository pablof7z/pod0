use std::path::PathBuf;

use pod0_domain::{
    AdSpanId, ChapterArtifact, ChapterArtifactId, ChapterId, CommandId, ContentDigest, EpisodeId,
    PodcastId, StateRevision,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyChapterSourceKind {
    ArtifactSqliteV0,
    ArtifactSqliteV1,
}

impl LegacyChapterSourceKind {
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
pub enum ChapterImportState {
    Staged,
    Verified,
    Imported,
    Corrupt,
    Discarded,
}

impl ChapterImportState {
    pub(crate) fn from_code(value: &str) -> Option<Self> {
        match value {
            "staged" => Some(Self::Staged),
            "verified" => Some(Self::Verified),
            "imported" => Some(Self::Imported),
            "corrupt" => Some(Self::Corrupt),
            "discarded" => Some(Self::Discarded),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChapterEvidenceKind {
    EpisodeAdjunct,
    WorkflowChapters,
    WorkflowAdSpans,
    AttemptManifest,
    UnreferencedChapterFile,
    UnreferencedAdFile,
}

impl ChapterEvidenceKind {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::EpisodeAdjunct => "episode_adjunct",
            Self::WorkflowChapters => "workflow_chapters",
            Self::WorkflowAdSpans => "workflow_ad_spans",
            Self::AttemptManifest => "attempt_manifest",
            Self::UnreferencedChapterFile => "unreferenced_chapter_file",
            Self::UnreferencedAdFile => "unreferenced_ad_file",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChapterEvidenceValidation {
    Canonical,
    Inert,
    Blocked,
}

impl ChapterEvidenceValidation {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::Inert => "inert",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterImportPlan {
    pub source_kind: LegacyChapterSourceKind,
    pub source_generation: u64,
    pub source_file_identity: ContentDigest,
    pub source_database_byte_count: u64,
    pub source_database_digest: ContentDigest,
    pub source_selection_digest: ContentDigest,
    pub evidence_count: u32,
    pub canonical_artifact_count: u32,
    pub selected_count: u32,
    pub blocked_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterBackupEvidence {
    pub database_digest: ContentDigest,
    pub database_byte_count: u64,
    pub file_count: u32,
    pub file_byte_count: u64,
    pub reused_database: bool,
    pub reused_files: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterImportReport {
    pub import_id: CommandId,
    pub plan: ChapterImportPlan,
    pub target_revision: StateRevision,
    pub backup: ChapterBackupEvidence,
    pub state: ChapterImportState,
    pub diagnostic_code: Option<String>,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterImportVerification {
    pub report: ChapterImportReport,
    pub verified_evidence_count: u32,
    pub verified_artifact_count: u32,
    pub verified_chapter_count: u64,
    pub verified_ad_span_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterRollbackExportReport {
    pub bundle_path: PathBuf,
    pub format_version: u32,
    pub core_schema_version: u32,
    pub source_generation: u64,
    pub evidence_count: u32,
    pub artifact_count: u32,
    pub bundle_digest: ContentDigest,
    pub reused_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InspectedChapterSource {
    pub(crate) plan: ChapterImportPlan,
    pub(crate) entries: Vec<InspectedChapterEvidence>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InspectedChapterEvidence {
    pub(crate) evidence_id: ContentDigest,
    pub(crate) kind: ChapterEvidenceKind,
    pub(crate) source_subject: String,
    pub(crate) episode_id: Option<EpisodeId>,
    pub(crate) podcast_id: Option<PodcastId>,
    pub(crate) source_row_id: Option<u64>,
    pub(crate) legacy_selected: Option<bool>,
    pub(crate) importer_selected: bool,
    pub(crate) source_input_version: Option<String>,
    pub(crate) source_output_version: Option<String>,
    pub(crate) source_origin: Option<String>,
    pub(crate) source_schema_version: Option<u32>,
    pub(crate) source_integrity: Option<String>,
    pub(crate) source_verified_at_ms: Option<i64>,
    pub(crate) source_path: Option<PathBuf>,
    pub(crate) source_row_digest: ContentDigest,
    pub(crate) raw_digest: ContentDigest,
    pub(crate) raw_byte_count: u64,
    pub(crate) raw_bytes: Vec<u8>,
    pub(crate) validation: ChapterEvidenceValidation,
    pub(crate) diagnostic_code: Option<String>,
    pub(crate) artifact: Option<ChapterArtifact>,
    pub(crate) legacy_chapters: Vec<LegacyChapterIdentity>,
    pub(crate) legacy_ad_spans: Vec<LegacyAdSpanIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyChapterIdentity {
    pub(crate) ordinal: u32,
    pub(crate) legacy_id: Option<[u8; 16]>,
    pub(crate) is_ai_generated: bool,
    pub(crate) chapter_id: Option<ChapterId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyAdSpanIdentity {
    pub(crate) ordinal: u32,
    pub(crate) legacy_id: Option<[u8; 16]>,
    pub(crate) ad_span_id: Option<AdSpanId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredChapterEvidence {
    pub(crate) evidence_id: ContentDigest,
    pub(crate) raw_digest: ContentDigest,
    pub(crate) raw_byte_count: u64,
    pub(crate) artifact_id: Option<ChapterArtifactId>,
    pub(crate) validation: ChapterEvidenceValidation,
}
