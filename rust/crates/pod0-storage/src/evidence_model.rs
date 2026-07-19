use pod0_domain::{EpisodeId, EvidenceChunkPolicy, EvidenceGenerationId, TranscriptVersionId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceGenerationState {
    Staged,
    Verified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvidenceGenerationSummary {
    pub generation_id: EvidenceGenerationId,
    pub transcript_version_id: TranscriptVersionId,
    pub episode_id: EpisodeId,
    pub artifact_schema_version: u32,
    pub policy: EvidenceChunkPolicy,
    pub segment_count: u32,
    pub span_count: u32,
    pub state: EvidenceGenerationState,
    pub staged_at_ms: i64,
    pub verified_at_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvidenceStageReceipt {
    pub generation_id: EvidenceGenerationId,
    pub already_present: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvidenceVerificationReceipt {
    pub generation_id: EvidenceGenerationId,
    pub already_verified: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvidenceSelectionReceipt {
    pub episode_id: EpisodeId,
    pub generation_id: EvidenceGenerationId,
    pub previous_generation_id: Option<EvidenceGenerationId>,
    pub already_selected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvidencePruneReceipt {
    pub generation_id: EvidenceGenerationId,
    pub pruned: bool,
}
