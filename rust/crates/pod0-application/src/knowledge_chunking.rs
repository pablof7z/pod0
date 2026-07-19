use pod0_domain::{EvidenceSpan, TranscriptSegmentRecord, TranscriptVersionRecord};

use crate::knowledge_chunking_policy::{
    compute_advance, dominant_speaker, snap_to_speaker_boundary,
};
use crate::knowledge_identity::{
    CanonicalSegment, SpanIdentity, evidence_span_id, transcript_content_digest,
    transcript_segment_id, transcript_version_id,
};
use crate::{
    EvidenceBuildError, EvidenceChunkPolicy, MAX_EVIDENCE_SPAN_TEXT_BYTES,
    MAX_PROVENANCE_PROVIDER_BYTES, MAX_SEGMENT_TEXT_BYTES, MAX_SOURCE_REVISION_BYTES,
    MAX_TRANSCRIPT_BYTES, MAX_TRANSCRIPT_SEGMENTS, TranscriptEvidenceArtifact,
    TranscriptEvidenceInput, provenance,
};

/// Pure deterministic transcript normalization and semantic span construction.
pub fn build_evidence_artifact(
    input: &TranscriptEvidenceInput,
    policy: EvidenceChunkPolicy,
) -> Result<TranscriptEvidenceArtifact, EvidenceBuildError> {
    validate_policy(policy)?;
    let source_revision = input.source_revision.trim();
    if source_revision.is_empty() {
        return Err(EvidenceBuildError::EmptySourceRevision);
    }
    if source_revision.len() > MAX_SOURCE_REVISION_BYTES {
        return Err(EvidenceBuildError::SourceRevisionTooLong);
    }
    if input
        .provider
        .as_ref()
        .is_some_and(|value| value.trim().len() > MAX_PROVENANCE_PROVIDER_BYTES)
    {
        return Err(EvidenceBuildError::ProviderTooLong);
    }
    if input.segments.len() > MAX_TRANSCRIPT_SEGMENTS {
        return Err(EvidenceBuildError::TooManySegments);
    }
    let canonical = normalize_segments(input)?;
    let content_digest = transcript_content_digest(&canonical);
    let provenance = provenance(input);
    let version_id = transcript_version_id(
        input.episode_id,
        input.podcast_id,
        source_revision,
        content_digest,
        &provenance,
    );
    let segments = canonical
        .iter()
        .map(|segment| TranscriptSegmentRecord {
            segment_id: transcript_segment_id(version_id, segment),
            ordinal: segment.ordinal,
            text: segment.text.clone(),
            start_milliseconds: segment.start_milliseconds,
            end_milliseconds: segment.end_milliseconds,
            speaker_id: segment.speaker_id,
        })
        .collect::<Vec<_>>();
    let version = TranscriptVersionRecord {
        transcript_version_id: version_id,
        episode_id: input.episode_id,
        podcast_id: input.podcast_id,
        source_revision: source_revision.to_owned(),
        content_digest,
        provenance: provenance.clone(),
    };
    let spans = build_spans(&version, &segments, policy)?;
    Ok(TranscriptEvidenceArtifact {
        version,
        segments,
        spans,
    })
}

#[must_use]
pub fn approximate_evidence_token_count(text: &str) -> usize {
    let words = text.split_whitespace().count();
    words.saturating_mul(13).saturating_add(9) / 10
}

fn normalize_segments(
    input: &TranscriptEvidenceInput,
) -> Result<Vec<CanonicalSegment>, EvidenceBuildError> {
    let mut normalized = Vec::with_capacity(input.segments.len());
    let mut total_bytes = 0_usize;
    let mut previous_start = None;
    for (index, segment) in input.segments.iter().enumerate() {
        let ordinal = u32::try_from(index).map_err(|_| EvidenceBuildError::TooManySegments)?;
        if segment.end_milliseconds < segment.start_milliseconds {
            return Err(EvidenceBuildError::InvalidSegmentTime { ordinal });
        }
        if previous_start.is_some_and(|start| segment.start_milliseconds < start) {
            return Err(EvidenceBuildError::SegmentsOutOfOrder { ordinal });
        }
        previous_start = Some(segment.start_milliseconds);
        let text = segment
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            continue;
        }
        if text.len() > MAX_SEGMENT_TEXT_BYTES {
            return Err(EvidenceBuildError::SegmentTextTooLong { ordinal });
        }
        total_bytes = total_bytes.saturating_add(text.len());
        if total_bytes > MAX_TRANSCRIPT_BYTES {
            return Err(EvidenceBuildError::TranscriptTooLarge);
        }
        normalized.push(CanonicalSegment {
            ordinal,
            text,
            start_milliseconds: segment.start_milliseconds,
            end_milliseconds: segment.end_milliseconds,
            speaker_id: segment.speaker_id,
        });
    }
    Ok(normalized)
}

fn build_spans(
    version: &TranscriptVersionRecord,
    segments: &[TranscriptSegmentRecord],
    policy: EvidenceChunkPolicy,
) -> Result<Vec<EvidenceSpan>, EvidenceBuildError> {
    if segments.is_empty() {
        return Ok(Vec::new());
    }
    let target = usize::from(policy.target_tokens);
    let overlap = target * usize::from(policy.overlap_per_mille) / 1_000;
    let snap_window = target * usize::from(policy.snap_tolerance_per_mille) / 1_000;
    let token_counts = segments
        .iter()
        .map(|segment| approximate_evidence_token_count(&segment.text))
        .collect::<Vec<_>>();
    let mut spans = Vec::new();
    let mut cursor = 0_usize;
    while cursor < segments.len() {
        let mut end = cursor;
        let mut running = 0_usize;
        while end < segments.len() && running.saturating_add(token_counts[end]) <= target {
            running = running.saturating_add(token_counts[end]);
            end += 1;
        }
        if end == cursor {
            end = cursor + 1;
        }
        end = snap_to_speaker_boundary(segments, &token_counts, cursor, end, snap_window);
        spans.push(make_span(version, &segments[cursor..end], policy)?);
        if spans.len() > MAX_TRANSCRIPT_SEGMENTS {
            return Err(EvidenceBuildError::TooManySpans);
        }
        if end == segments.len() {
            // The final span already contains the overlap tail; do not emit a
            // redundant suffix-only span after reaching the transcript end.
            break;
        }
        cursor += compute_advance(&token_counts, cursor, end, overlap).max(1);
    }
    Ok(spans)
}

fn make_span(
    version: &TranscriptVersionRecord,
    segments: &[TranscriptSegmentRecord],
    policy: EvidenceChunkPolicy,
) -> Result<EvidenceSpan, EvidenceBuildError> {
    let first = &segments[0];
    let last = &segments[segments.len() - 1];
    let text = segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if text.len() > MAX_EVIDENCE_SPAN_TEXT_BYTES {
        return Err(EvidenceBuildError::SpanTextTooLong);
    }
    let end_milliseconds = segments
        .iter()
        .map(|segment| segment.end_milliseconds)
        .max()
        .unwrap_or(first.end_milliseconds);
    let end_ordinal_exclusive = last.ordinal.saturating_add(1);
    let span_id = evidence_span_id(SpanIdentity {
        transcript_version_id: version.transcript_version_id,
        content_digest: version.content_digest,
        policy_version: policy.version,
        target_tokens: policy.target_tokens,
        overlap_per_mille: policy.overlap_per_mille,
        snap_tolerance_per_mille: policy.snap_tolerance_per_mille,
        first_segment_id: first.segment_id,
        last_segment_id: last.segment_id,
        start_ordinal: first.ordinal,
        end_ordinal_exclusive,
        start_milliseconds: first.start_milliseconds,
        end_milliseconds,
        text: &text,
    });
    Ok(EvidenceSpan {
        span_id,
        transcript_version_id: version.transcript_version_id,
        transcript_content_digest: version.content_digest,
        episode_id: version.episode_id,
        podcast_id: version.podcast_id,
        first_segment_id: first.segment_id,
        last_segment_id: last.segment_id,
        start_segment_ordinal: first.ordinal,
        end_segment_ordinal_exclusive: end_ordinal_exclusive,
        start_milliseconds: first.start_milliseconds,
        end_milliseconds,
        text,
        speaker_id: dominant_speaker(segments),
        provenance: version.provenance.clone(),
        chunk_policy_version: policy.version,
    })
}

fn validate_policy(policy: EvidenceChunkPolicy) -> Result<(), EvidenceBuildError> {
    if policy.version == 0
        || !(20..=4_096).contains(&policy.target_tokens)
        || policy.overlap_per_mille > 500
        || policy.snap_tolerance_per_mille > 500
    {
        return Err(EvidenceBuildError::InvalidPolicy);
    }
    Ok(())
}
