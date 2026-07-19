use pod0_domain::{ContentDigest, EpisodeId, EvidenceGenerationId, EvidenceSpanId, PodcastId};

pub const MAX_EVIDENCE_INDEX_PAGE_ITEMS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum EvidenceIndexStage {
    Ready,
    Missing,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EvidenceIndexProjection {
    pub episode_id: EpisodeId,
    pub stage: EvidenceIndexStage,
    pub generation_id: Option<EvidenceGenerationId>,
    pub transcript_content_digest: Option<ContentDigest>,
    pub spans: Vec<EvidenceIndexSpanProjection>,
    pub total_spans: u32,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EvidenceIndexSpanProjection {
    pub span_id: EvidenceSpanId,
    pub generation_id: EvidenceGenerationId,
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub text: String,
}
