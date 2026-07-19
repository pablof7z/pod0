use crate::knowledge_identity::StableHash;
use crate::{
    ContentDigest, EvidenceChunkPolicy, EvidenceSpan, TranscriptProvenance,
    TranscriptSegmentRecord, TranscriptVersionRecord,
};

pub(crate) fn evidence_artifact_digest(
    schema_version: u32,
    policy: EvidenceChunkPolicy,
    version: &TranscriptVersionRecord,
    segments: &[TranscriptSegmentRecord],
    spans: &[EvidenceSpan],
) -> ContentDigest {
    let mut hash = StableHash::new(b"pod0.evidence-artifact.v1");
    hash.u32(schema_version);
    hash.u32(policy.version);
    hash.u16(policy.target_tokens);
    hash.u16(policy.overlap_per_mille);
    hash.u16(policy.snap_tolerance_per_mille);
    hash.bytes(&version.transcript_version_id.into_bytes());
    hash.bytes(&version.episode_id.into_bytes());
    hash.bytes(&version.podcast_id.into_bytes());
    hash.text(&version.source_revision);
    hash.bytes(&version.content_digest.into_bytes());
    hash_provenance(&mut hash, &version.provenance);
    hash.u64(segments.len() as u64);
    for segment in segments {
        hash.bytes(&segment.segment_id.into_bytes());
        hash.u32(segment.ordinal);
        hash.text(&segment.text);
        hash.u64(segment.start_milliseconds);
        hash.u64(segment.end_milliseconds);
        hash.optional_id(segment.speaker_id.map(crate::SpeakerId::into_bytes));
    }
    hash.u64(spans.len() as u64);
    for span in spans {
        hash.bytes(&span.span_id.into_bytes());
        hash.bytes(&span.transcript_version_id.into_bytes());
        hash.bytes(&span.transcript_content_digest.into_bytes());
        hash.bytes(&span.episode_id.into_bytes());
        hash.bytes(&span.podcast_id.into_bytes());
        hash.bytes(&span.first_segment_id.into_bytes());
        hash.bytes(&span.last_segment_id.into_bytes());
        hash.u32(span.start_segment_ordinal);
        hash.u32(span.end_segment_ordinal_exclusive);
        hash.u64(span.start_milliseconds);
        hash.u64(span.end_milliseconds);
        hash.text(&span.text);
        hash.optional_id(span.speaker_id.map(crate::SpeakerId::into_bytes));
        hash_provenance(&mut hash, &span.provenance);
        hash.u32(span.chunk_policy_version);
    }
    ContentDigest::from_bytes(hash.finish())
}

fn hash_provenance(hash: &mut StableHash, provenance: &TranscriptProvenance) {
    hash.transcript_source(provenance.source);
    hash.optional_text(provenance.provider.as_deref());
    hash.bytes(&provenance.source_payload_digest.into_bytes());
}
