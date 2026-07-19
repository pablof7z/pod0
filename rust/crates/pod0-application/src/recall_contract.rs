use pod0_domain::{
    ContentDigest, EpisodeId, EvidenceGenerationId, EvidenceSpanId, PodcastId, RecallQueryId,
    SpeakerId, TranscriptProvenance, TranscriptSegmentId, TranscriptVersionId,
};

use crate::{CoreFailure, OperationProjection};

pub const MAX_RECALL_QUERY_BYTES: usize = 512;
pub const MAX_RECALL_EMBEDDING_DIMENSIONS: usize = 4_096;
pub const MAX_RECALL_CANDIDATES: usize = 512;
pub const MAX_RECALL_EVIDENCE: usize = 20;
pub const MAX_RECALL_EXCERPT_BYTES: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RecallQuery {
    pub query_id: RecallQueryId,
    pub text: String,
    pub scope: RecallScope,
    pub limit: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum RecallScope {
    Library,
    Podcast { podcast_id: PodcastId },
    Episode { episode_id: EpisodeId },
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum RecallStage {
    Queued,
    Running { phase: RecallPhase },
    Ready,
    NoEvidence,
    IndexUnavailable,
    Cancelled,
    Failed,
    Unsupported { wire_code: u32 },
}

impl RecallStage {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Ready
                | Self::NoEvidence
                | Self::IndexUnavailable
                | Self::Cancelled
                | Self::Failed
                | Self::Unsupported { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum RecallPhase {
    Retrieving,
    Reranking,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RecallResultProjection {
    pub query_id: RecallQueryId,
    pub stage: RecallStage,
    pub evidence: Vec<RecallEvidenceProjection>,
    pub failure: Option<CoreFailure>,
    pub operation: Option<OperationProjection>,
}

impl RecallResultProjection {
    pub fn enforce_bounds(&mut self, requested_items: usize) {
        self.evidence
            .truncate(requested_items.clamp(1, MAX_RECALL_EVIDENCE));
        for evidence in &mut self.evidence {
            evidence.excerpt = bounded_recall_text(&evidence.excerpt, MAX_RECALL_EXCERPT_BYTES);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RecallEvidenceProjection {
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub generation_id: EvidenceGenerationId,
    pub transcript_version_id: TranscriptVersionId,
    pub transcript_content_digest: ContentDigest,
    pub span_id: EvidenceSpanId,
    pub first_segment_id: TranscriptSegmentId,
    pub last_segment_id: TranscriptSegmentId,
    pub start_segment_ordinal: u32,
    pub end_segment_ordinal_exclusive: u32,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub excerpt: String,
    pub speaker_id: Option<SpeakerId>,
    pub provenance: TranscriptProvenance,
    pub score: RecallScoreProjection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RecallScoreProjection {
    pub vector_rrf_units: u64,
    pub lexical_rrf_units: u64,
    pub total_rrf_units: u64,
    pub base_rank: u16,
    pub rerank_rank: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RecallEmbeddingVector {
    /// Provider dimensions quantized to signed millionths at the host boundary.
    pub values: Vec<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RecallCandidateObservation {
    pub episode_id: EpisodeId,
    pub generation_id: EvidenceGenerationId,
    pub span_id: EvidenceSpanId,
    pub vector_rank: Option<u16>,
    pub lexical_rank: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RecallRerankDocument {
    pub span_id: EvidenceSpanId,
    pub excerpt: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RecallRerankObservation {
    pub span_id: EvidenceSpanId,
    pub rank: u16,
}

#[must_use]
pub fn bounded_recall_text(value: &str, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value.to_owned();
    }
    let suffix = "…";
    let mut end = maximum_bytes.saturating_sub(suffix.len());
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{}{}", &value[..end], suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn recall_text_and_projection_bounds_preserve_valid_utf8() {
        let bounded = bounded_recall_text(&"é".repeat(3_000), 4_096);
        assert!(bounded.len() <= 4_096);
        assert!(bounded.ends_with('…'));

        let query_id = RecallQueryId::from_parts(0, 1);
        let mut projection = RecallResultProjection {
            query_id,
            stage: RecallStage::Ready,
            evidence: Vec::new(),
            failure: None,
            operation: None,
        };
        projection.enforce_bounds(0);
        assert!(projection.evidence.is_empty());
        assert!(RecallStage::Ready.is_terminal());
        assert!(!RecallStage::Queued.is_terminal());
    }

    #[test]
    fn cross_language_recall_fixture_matches_the_typed_contract() {
        let values =
            include_str!("../../../../Fixtures/CoreKnowledge/recall-projection-v1.properties")
                .lines()
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .filter_map(|line| line.split_once('='))
                .collect::<BTreeMap<_, _>>();
        let number = |key: &str| values[key].parse::<u64>().unwrap();

        assert_eq!(values["fixture_version"], "1");
        assert_eq!(
            number("contract_version"),
            u64::from(crate::FACADE_CONTRACT_VERSION)
        );
        assert_eq!(values["stage"], "ready");
        assert_eq!(
            RecallQueryId::from_parts(number("query_id_high"), number("query_id_low")),
            RecallQueryId::from_parts(42, 7)
        );
        assert_eq!(number("start_milliseconds"), 47_125);
        assert_eq!(number("end_milliseconds"), 60_000);
        assert_eq!(
            number("vector_rrf_units") + number("lexical_rrf_units"),
            number("total_rrf_units")
        );
        assert_eq!(values["provenance_source"], "publisher");
        assert_eq!(
            values["excerpt"],
            "Small habits become durable when the cue is obvious."
        );
    }
}
