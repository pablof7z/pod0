use crate::{
    EpisodeId, EvidenceGenerationId, EvidenceSpanId, PodcastId, SpeakerId, TranscriptSegmentId,
    TranscriptSource, TranscriptVersionId,
};
use std::fmt;

pub const EVIDENCE_ARTIFACT_SCHEMA_VERSION: u32 = 1;
pub const EVIDENCE_CHUNK_POLICY_VERSION: u32 = 1;
pub const MAX_TRANSCRIPT_SEGMENTS: usize = 50_000;
pub const MAX_TRANSCRIPT_BYTES: usize = 16 * 1_024 * 1_024;
pub const MAX_SEGMENT_TEXT_BYTES: usize = 16_384;
pub const MAX_EVIDENCE_SPAN_TEXT_BYTES: usize = 65_536;
pub const MAX_SOURCE_REVISION_BYTES: usize = 256;
pub const MAX_PROVENANCE_PROVIDER_BYTES: usize = 128;

/// Exact SHA-256 value represented without a stringly typed hex boundary.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    uniffi::Record,
)]
pub struct ContentDigest {
    pub word_0: u64,
    pub word_1: u64,
    pub word_2: u64,
    pub word_3: u64,
}

impl ContentDigest {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self {
            word_0: u64::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
            word_1: u64::from_be_bytes([
                bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14],
                bytes[15],
            ]),
            word_2: u64::from_be_bytes([
                bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22],
                bytes[23],
            ]),
            word_3: u64::from_be_bytes([
                bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30],
                bytes[31],
            ]),
        }
    }

    #[must_use]
    pub const fn into_bytes(self) -> [u8; 32] {
        let first = self.word_0.to_be_bytes();
        let second = self.word_1.to_be_bytes();
        let third = self.word_2.to_be_bytes();
        let fourth = self.word_3.to_be_bytes();
        [
            first[0], first[1], first[2], first[3], first[4], first[5], first[6], first[7],
            second[0], second[1], second[2], second[3], second[4], second[5], second[6], second[7],
            third[0], third[1], third[2], third[3], third[4], third[5], third[6], third[7],
            fourth[0], fourth[1], fourth[2], fourth[3], fourth[4], fourth[5], fourth[6], fourth[7],
        ]
    }
}

/// Provenance facts copied from the selected transcript artifact. The source
/// payload digest protects identity without leaking a URL or provider body.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct TranscriptProvenance {
    pub source: TranscriptSource,
    pub provider: Option<String>,
    pub source_payload_digest: ContentDigest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptVersionRecord {
    pub transcript_version_id: TranscriptVersionId,
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub source_revision: String,
    pub content_digest: ContentDigest,
    pub provenance: TranscriptProvenance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptSegmentRecord {
    pub segment_id: TranscriptSegmentId,
    pub ordinal: u32,
    pub text: String,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub speaker_id: Option<SpeakerId>,
}

/// A normalized, playable retrieval unit. Segment bounds are inclusive on the
/// first ID and exclusive on the ordinal end, avoiding ambiguous overlap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceSpan {
    pub span_id: EvidenceSpanId,
    pub transcript_version_id: TranscriptVersionId,
    pub transcript_content_digest: ContentDigest,
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub first_segment_id: TranscriptSegmentId,
    pub last_segment_id: TranscriptSegmentId,
    pub start_segment_ordinal: u32,
    pub end_segment_ordinal_exclusive: u32,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub text: String,
    pub speaker_id: Option<SpeakerId>,
    pub provenance: TranscriptProvenance,
    pub chunk_policy_version: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvidenceScoreComponents {
    pub vector_rrf_units: u64,
    pub lexical_rrf_units: u64,
    pub total_rrf_units: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RankedEvidenceReference {
    pub span_id: EvidenceSpanId,
    pub score: EvidenceScoreComponents,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
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
    pub schema_version: u32,
    pub generation_id: EvidenceGenerationId,
    pub integrity_digest: ContentDigest,
    pub policy: EvidenceChunkPolicy,
    pub version: TranscriptVersionRecord,
    pub segments: Vec<TranscriptSegmentRecord>,
    pub spans: Vec<EvidenceSpan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceArtifactError {
    NewerSchema { stored: u32, supported: u32 },
    InvalidSchema,
    InvalidPolicy,
    CollectionLimit,
    TextLimit,
    InvalidText,
    InvalidTime,
    InvalidOrdering,
    IdentityMismatch,
    ReferenceMismatch,
    Incomplete,
}

impl fmt::Display for EvidenceArtifactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid evidence artifact: {self:?}")
    }
}

impl std::error::Error for EvidenceArtifactError {}
