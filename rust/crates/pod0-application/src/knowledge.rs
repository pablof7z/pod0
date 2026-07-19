use pod0_domain::{
    ContentDigest, EpisodeId, EvidenceSpanId, PodcastId, SpeakerId, TranscriptProvenance,
    TranscriptSource,
};

pub use pod0_domain::{
    EVIDENCE_ARTIFACT_SCHEMA_VERSION, EVIDENCE_CHUNK_POLICY_VERSION, EvidenceChunkPolicy,
    MAX_EVIDENCE_SPAN_TEXT_BYTES, MAX_PROVENANCE_PROVIDER_BYTES, MAX_SEGMENT_TEXT_BYTES,
    MAX_SOURCE_REVISION_BYTES, MAX_TRANSCRIPT_BYTES, MAX_TRANSCRIPT_SEGMENTS,
    TranscriptEvidenceArtifact,
};
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
    ArtifactInvariant,
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
