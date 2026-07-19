use std::collections::{BTreeMap, BTreeSet};

use crate::knowledge_artifact_hash::evidence_artifact_digest;
use crate::knowledge_identity::first_16;
use crate::{
    CanonicalTranscriptSegment, EVIDENCE_ARTIFACT_SCHEMA_VERSION, EvidenceArtifactError,
    EvidenceChunkPolicy, EvidenceGenerationId, EvidenceSpan, MAX_EVIDENCE_SPAN_TEXT_BYTES,
    MAX_PROVENANCE_PROVIDER_BYTES, MAX_SEGMENT_TEXT_BYTES, MAX_SOURCE_REVISION_BYTES,
    MAX_TRANSCRIPT_BYTES, MAX_TRANSCRIPT_SEGMENTS, SpanIdentity, SpeakerId,
    TranscriptEvidenceArtifact, TranscriptSegmentRecord, TranscriptVersionRecord, evidence_span_id,
    transcript_content_digest, transcript_segment_id, transcript_version_id,
};

impl TranscriptEvidenceArtifact {
    pub fn seal(
        policy: EvidenceChunkPolicy,
        version: TranscriptVersionRecord,
        segments: Vec<TranscriptSegmentRecord>,
        spans: Vec<EvidenceSpan>,
    ) -> Result<Self, EvidenceArtifactError> {
        let integrity_digest = evidence_artifact_digest(
            EVIDENCE_ARTIFACT_SCHEMA_VERSION,
            policy,
            &version,
            &segments,
            &spans,
        );
        let artifact = Self {
            schema_version: EVIDENCE_ARTIFACT_SCHEMA_VERSION,
            generation_id: EvidenceGenerationId::from_bytes(first_16(
                integrity_digest.into_bytes(),
            )),
            integrity_digest,
            policy,
            version,
            segments,
            spans,
        };
        artifact.verify_integrity()?;
        Ok(artifact)
    }

    pub fn verify_integrity(&self) -> Result<(), EvidenceArtifactError> {
        validate_schema_and_policy(self)?;
        let canonical = validate_segments(&self.version, &self.segments)?;
        validate_spans(self, &canonical)?;
        let digest = evidence_artifact_digest(
            self.schema_version,
            self.policy,
            &self.version,
            &self.segments,
            &self.spans,
        );
        if digest != self.integrity_digest
            || EvidenceGenerationId::from_bytes(first_16(digest.into_bytes())) != self.generation_id
        {
            return Err(EvidenceArtifactError::IdentityMismatch);
        }
        Ok(())
    }
}

fn validate_schema_and_policy(
    artifact: &TranscriptEvidenceArtifact,
) -> Result<(), EvidenceArtifactError> {
    if artifact.schema_version > EVIDENCE_ARTIFACT_SCHEMA_VERSION {
        return Err(EvidenceArtifactError::NewerSchema {
            stored: artifact.schema_version,
            supported: EVIDENCE_ARTIFACT_SCHEMA_VERSION,
        });
    }
    if artifact.schema_version != EVIDENCE_ARTIFACT_SCHEMA_VERSION {
        return Err(EvidenceArtifactError::InvalidSchema);
    }
    let policy = artifact.policy;
    if policy.version == 0
        || !(20..=4_096).contains(&policy.target_tokens)
        || policy.overlap_per_mille > 500
        || policy.snap_tolerance_per_mille > 500
    {
        return Err(EvidenceArtifactError::InvalidPolicy);
    }
    if artifact.segments.len() > MAX_TRANSCRIPT_SEGMENTS
        || artifact.spans.len() > MAX_TRANSCRIPT_SEGMENTS
    {
        return Err(EvidenceArtifactError::CollectionLimit);
    }
    let revision = &artifact.version.source_revision;
    if revision.is_empty()
        || revision.len() > MAX_SOURCE_REVISION_BYTES
        || revision.trim() != revision
    {
        return Err(EvidenceArtifactError::InvalidText);
    }
    if artifact
        .version
        .provenance
        .provider
        .as_ref()
        .is_some_and(|value| {
            value.is_empty() || value.len() > MAX_PROVENANCE_PROVIDER_BYTES || value.trim() != value
        })
    {
        return Err(EvidenceArtifactError::InvalidText);
    }
    Ok(())
}

fn validate_segments(
    version: &TranscriptVersionRecord,
    segments: &[TranscriptSegmentRecord],
) -> Result<Vec<CanonicalTranscriptSegment>, EvidenceArtifactError> {
    let mut total_bytes = 0_usize;
    let mut previous = None;
    let mut ids = BTreeSet::new();
    let mut canonical = Vec::with_capacity(segments.len());
    for segment in segments {
        if segment.text.is_empty()
            || segment.text.len() > MAX_SEGMENT_TEXT_BYTES
            || segment
                .text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                != segment.text
        {
            return Err(EvidenceArtifactError::InvalidText);
        }
        total_bytes = total_bytes.saturating_add(segment.text.len());
        if total_bytes > MAX_TRANSCRIPT_BYTES {
            return Err(EvidenceArtifactError::TextLimit);
        }
        if segment.end_milliseconds < segment.start_milliseconds {
            return Err(EvidenceArtifactError::InvalidTime);
        }
        if previous.is_some_and(|(ordinal, start)| {
            segment.ordinal <= ordinal || segment.start_milliseconds < start
        }) {
            return Err(EvidenceArtifactError::InvalidOrdering);
        }
        previous = Some((segment.ordinal, segment.start_milliseconds));
        if !ids.insert(segment.segment_id) {
            return Err(EvidenceArtifactError::IdentityMismatch);
        }
        canonical.push(CanonicalTranscriptSegment {
            ordinal: segment.ordinal,
            text: segment.text.clone(),
            start_milliseconds: segment.start_milliseconds,
            end_milliseconds: segment.end_milliseconds,
            speaker_id: segment.speaker_id,
        });
    }
    if transcript_content_digest(&canonical) != version.content_digest
        || transcript_version_id(
            version.episode_id,
            version.podcast_id,
            &version.source_revision,
            version.content_digest,
            &version.provenance,
        ) != version.transcript_version_id
        || segments.iter().zip(&canonical).any(|(record, content)| {
            transcript_segment_id(version.transcript_version_id, content) != record.segment_id
        })
    {
        return Err(EvidenceArtifactError::IdentityMismatch);
    }
    Ok(canonical)
}

fn validate_spans(
    artifact: &TranscriptEvidenceArtifact,
    _: &[CanonicalTranscriptSegment],
) -> Result<(), EvidenceArtifactError> {
    if artifact.segments.is_empty() {
        return if artifact.spans.is_empty() {
            Ok(())
        } else {
            Err(EvidenceArtifactError::ReferenceMismatch)
        };
    }
    if artifact.spans.is_empty() {
        return Err(EvidenceArtifactError::Incomplete);
    }
    let positions = artifact
        .segments
        .iter()
        .enumerate()
        .map(|(index, segment)| (segment.segment_id, index))
        .collect::<BTreeMap<_, _>>();
    let mut span_ids = BTreeSet::new();
    let mut previous_start = None;
    let mut last_end = None;
    for span in &artifact.spans {
        let start = *positions
            .get(&span.first_segment_id)
            .ok_or(EvidenceArtifactError::ReferenceMismatch)?;
        let end = *positions
            .get(&span.last_segment_id)
            .ok_or(EvidenceArtifactError::ReferenceMismatch)?;
        if start > end || previous_start.is_some_and(|value| start <= value) {
            return Err(EvidenceArtifactError::InvalidOrdering);
        }
        previous_start = Some(start);
        last_end = Some(end);
        validate_span(artifact, span, &artifact.segments[start..=end])?;
        if !span_ids.insert(span.span_id) {
            return Err(EvidenceArtifactError::IdentityMismatch);
        }
    }
    if positions[&artifact.spans[0].first_segment_id] != 0
        || last_end != Some(artifact.segments.len() - 1)
    {
        return Err(EvidenceArtifactError::Incomplete);
    }
    Ok(())
}

fn validate_span(
    artifact: &TranscriptEvidenceArtifact,
    span: &EvidenceSpan,
    segments: &[TranscriptSegmentRecord],
) -> Result<(), EvidenceArtifactError> {
    let first = &segments[0];
    let last = &segments[segments.len() - 1];
    let text = segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if text.len() > MAX_EVIDENCE_SPAN_TEXT_BYTES {
        return Err(EvidenceArtifactError::TextLimit);
    }
    let end_milliseconds = segments
        .iter()
        .map(|segment| segment.end_milliseconds)
        .max()
        .ok_or(EvidenceArtifactError::ReferenceMismatch)?;
    if span.transcript_version_id != artifact.version.transcript_version_id
        || span.transcript_content_digest != artifact.version.content_digest
        || span.episode_id != artifact.version.episode_id
        || span.podcast_id != artifact.version.podcast_id
        || span.start_segment_ordinal != first.ordinal
        || span.end_segment_ordinal_exclusive != last.ordinal.saturating_add(1)
        || span.start_milliseconds != first.start_milliseconds
        || span.end_milliseconds != end_milliseconds
        || span.text != text
        || span.speaker_id != dominant_speaker(segments)
        || span.provenance != artifact.version.provenance
        || span.chunk_policy_version != artifact.policy.version
    {
        return Err(EvidenceArtifactError::ReferenceMismatch);
    }
    let expected = evidence_span_id(SpanIdentity {
        transcript_version_id: span.transcript_version_id,
        content_digest: span.transcript_content_digest,
        policy_version: artifact.policy.version,
        target_tokens: artifact.policy.target_tokens,
        overlap_per_mille: artifact.policy.overlap_per_mille,
        snap_tolerance_per_mille: artifact.policy.snap_tolerance_per_mille,
        first_segment_id: first.segment_id,
        last_segment_id: last.segment_id,
        start_ordinal: first.ordinal,
        end_ordinal_exclusive: last.ordinal.saturating_add(1),
        start_milliseconds: first.start_milliseconds,
        end_milliseconds,
        text: &text,
    });
    if expected == span.span_id {
        Ok(())
    } else {
        Err(EvidenceArtifactError::IdentityMismatch)
    }
}

fn dominant_speaker(segments: &[TranscriptSegmentRecord]) -> Option<SpeakerId> {
    let mut totals = BTreeMap::<SpeakerId, usize>::new();
    for segment in segments {
        if let Some(speaker_id) = segment.speaker_id {
            let words = segment.text.split_whitespace().count();
            let tokens = words.saturating_mul(13).saturating_add(9) / 10;
            *totals.entry(speaker_id).or_default() += tokens;
        }
    }
    totals
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
        .map(|(speaker_id, _)| speaker_id)
}
