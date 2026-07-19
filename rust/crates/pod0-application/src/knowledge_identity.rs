use pod0_domain::{
    ContentDigest, EpisodeId, EvidenceSpanId, PodcastId, SpeakerId, TranscriptProvenance,
    TranscriptSegmentId, TranscriptSource, TranscriptVersionId,
};
use sha2::{Digest as _, Sha256};

pub(crate) struct CanonicalSegment {
    pub ordinal: u32,
    pub text: String,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub speaker_id: Option<SpeakerId>,
}

pub(crate) struct SpanIdentity<'a> {
    pub transcript_version_id: TranscriptVersionId,
    pub content_digest: ContentDigest,
    pub policy_version: u32,
    pub target_tokens: u16,
    pub overlap_per_mille: u16,
    pub snap_tolerance_per_mille: u16,
    pub first_segment_id: TranscriptSegmentId,
    pub last_segment_id: TranscriptSegmentId,
    pub start_ordinal: u32,
    pub end_ordinal_exclusive: u32,
    pub start_milliseconds: u64,
    pub end_milliseconds: u64,
    pub text: &'a str,
}

pub(crate) fn transcript_content_digest(segments: &[CanonicalSegment]) -> ContentDigest {
    let mut hash = StableHash::new(b"pod0.transcript-content.v1");
    hash.u64(segments.len() as u64);
    for segment in segments {
        hash.u32(segment.ordinal);
        hash.text(&segment.text);
        hash.u64(segment.start_milliseconds);
        hash.u64(segment.end_milliseconds);
        hash.optional_id(segment.speaker_id.map(SpeakerId::into_bytes));
    }
    ContentDigest::from_bytes(hash.finish())
}

pub(crate) fn transcript_version_id(
    episode_id: EpisodeId,
    podcast_id: PodcastId,
    source_revision: &str,
    content_digest: ContentDigest,
    provenance: &TranscriptProvenance,
) -> TranscriptVersionId {
    let mut hash = StableHash::new(b"pod0.transcript-version.v1");
    hash.bytes(&episode_id.into_bytes());
    hash.bytes(&podcast_id.into_bytes());
    hash.text(source_revision);
    hash.bytes(&content_digest.into_bytes());
    hash.transcript_source(provenance.source);
    match provenance.provider.as_deref() {
        Some(provider) => {
            hash.u8(1);
            hash.text(provider);
        }
        None => hash.u8(0),
    }
    hash.bytes(&provenance.source_payload_digest.into_bytes());
    TranscriptVersionId::from_bytes(first_16(hash.finish()))
}

pub(crate) fn transcript_segment_id(
    version_id: TranscriptVersionId,
    segment: &CanonicalSegment,
) -> TranscriptSegmentId {
    let mut hash = StableHash::new(b"pod0.transcript-segment.v1");
    hash.bytes(&version_id.into_bytes());
    hash.u32(segment.ordinal);
    hash.text(&segment.text);
    hash.u64(segment.start_milliseconds);
    hash.u64(segment.end_milliseconds);
    hash.optional_id(segment.speaker_id.map(SpeakerId::into_bytes));
    TranscriptSegmentId::from_bytes(first_16(hash.finish()))
}

pub(crate) fn evidence_span_id(input: SpanIdentity<'_>) -> EvidenceSpanId {
    let mut hash = StableHash::new(b"pod0.evidence-span.v1");
    hash.bytes(&input.transcript_version_id.into_bytes());
    hash.bytes(&input.content_digest.into_bytes());
    hash.u32(input.policy_version);
    hash.u16(input.target_tokens);
    hash.u16(input.overlap_per_mille);
    hash.u16(input.snap_tolerance_per_mille);
    hash.bytes(&input.first_segment_id.into_bytes());
    hash.bytes(&input.last_segment_id.into_bytes());
    hash.u32(input.start_ordinal);
    hash.u32(input.end_ordinal_exclusive);
    hash.u64(input.start_milliseconds);
    hash.u64(input.end_milliseconds);
    hash.text(input.text);
    EvidenceSpanId::from_bytes(first_16(hash.finish()))
}

fn first_16(bytes: [u8; 32]) -> [u8; 16] {
    let mut result = [0_u8; 16];
    result.copy_from_slice(&bytes[..16]);
    result
}

struct StableHash(Sha256);

impl StableHash {
    fn new(domain: &[u8]) -> Self {
        let mut value = Self(Sha256::new());
        value.bytes(domain);
        value
    }

    fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }

    fn u8(&mut self, value: u8) {
        self.0.update([value]);
    }

    fn u16(&mut self, value: u16) {
        self.0.update(value.to_be_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.0.update(value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.update(value.to_be_bytes());
    }

    fn bytes(&mut self, value: &[u8]) {
        self.u64(value.len() as u64);
        self.0.update(value);
    }

    fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn optional_id(&mut self, value: Option<[u8; 16]>) {
        match value {
            Some(value) => {
                self.u8(1);
                self.bytes(&value);
            }
            None => self.u8(0),
        }
    }

    fn transcript_source(&mut self, source: TranscriptSource) {
        match source {
            TranscriptSource::Publisher => self.u8(1),
            TranscriptSource::Scribe => self.u8(2),
            TranscriptSource::Whisper => self.u8(3),
            TranscriptSource::OnDevice => self.u8(4),
            TranscriptSource::AssemblyAi => self.u8(5),
            TranscriptSource::Other => self.u8(6),
            TranscriptSource::Unsupported { wire_code } => {
                self.u8(u8::MAX);
                self.u32(wire_code);
            }
        }
    }
}
