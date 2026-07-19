use pod0_domain::{
    ContentDigest, EpisodeId, EvidenceSpan, EvidenceSpanId, PodcastId, SpeakerId,
    TranscriptProvenance, TranscriptSegmentRecord, TranscriptSource, TranscriptVersionRecord,
};

pub const EVIDENCE_CHUNK_POLICY_VERSION: u32 = 1;
pub const MAX_TRANSCRIPT_SEGMENTS: usize = 50_000;
pub const MAX_TRANSCRIPT_BYTES: usize = 16 * 1_024 * 1_024;
pub const MAX_SEGMENT_TEXT_BYTES: usize = 16_384;
pub const MAX_EVIDENCE_SPAN_TEXT_BYTES: usize = 65_536;
pub const MAX_SOURCE_REVISION_BYTES: usize = 256;
pub const MAX_PROVENANCE_PROVIDER_BYTES: usize = 128;
pub const MAX_RANK_CANDIDATES: usize = 512;
pub const MAX_RANKED_EVIDENCE: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptSegmentInput {
    pub text: String,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub speaker_id: Option<SpeakerId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptEvidenceInput {
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub source_revision: String,
    pub source: TranscriptSource,
    pub provider: Option<String>,
    pub source_payload_digest: ContentDigest,
    pub segments: Vec<TranscriptSegmentInput>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvidenceChunkPolicy {
    pub version: u32,
    pub target_tokens: u16,
    pub overlap_per_mille: u16,
    pub snap_tolerance_per_mille: u16,
}

impl Default for EvidenceChunkPolicy {
    fn default() -> Self {
        Self {
            version: EVIDENCE_CHUNK_POLICY_VERSION,
            target_tokens: 400,
            overlap_per_mille: 150,
            snap_tolerance_per_mille: 200,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptEvidenceArtifact {
    pub version: TranscriptVersionRecord,
    pub segments: Vec<TranscriptSegmentRecord>,
    pub spans: Vec<EvidenceSpan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvidenceCandidateObservation {
    pub span_id: EvidenceSpanId,
    /// One-based rank from the raw vector capability result.
    pub vector_rank: Option<u16>,
    /// One-based rank from the raw lexical capability result.
    pub lexical_rank: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvidenceBuildError {
    EmptySourceRevision,
    SourceRevisionTooLong,
    ProviderTooLong,
    InvalidPolicy,
    TooManySegments,
    SegmentTextTooLong { ordinal: u32 },
    InvalidSegmentTime { ordinal: u32 },
    SegmentsOutOfOrder { ordinal: u32 },
    TranscriptTooLarge,
    SpanTextTooLong,
    TooManySpans,
}

impl std::fmt::Display for EvidenceBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid transcript evidence input: {self:?}")
    }
}

impl std::error::Error for EvidenceBuildError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvidenceRankingError {
    EmptyLimit,
    LimitTooLarge,
    TooManyCandidates,
    CandidateHasNoRank { span_id: EvidenceSpanId },
    DuplicateCandidate { span_id: EvidenceSpanId },
    InvalidVectorRank { rank: u16 },
    InvalidLexicalRank { rank: u16 },
    DuplicateVectorRank { rank: u16 },
    DuplicateLexicalRank { rank: u16 },
}

impl std::fmt::Display for EvidenceRankingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid evidence ranking input: {self:?}")
    }
}

impl std::error::Error for EvidenceRankingError {}

pub(crate) fn provenance(input: &TranscriptEvidenceInput) -> TranscriptProvenance {
    TranscriptProvenance {
        source: input.source,
        provider: input
            .provider
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        source_payload_digest: input.source_payload_digest,
    }
}
