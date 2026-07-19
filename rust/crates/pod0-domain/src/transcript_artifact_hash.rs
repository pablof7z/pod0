use crate::knowledge_identity::StableHash;
use crate::{ContentDigest, TranscriptArtifact, TranscriptSource};

pub(crate) fn transcript_artifact_digest(artifact: &TranscriptArtifact) -> ContentDigest {
    let mut hash = StableHash::new(b"pod0.transcript-artifact.v1");
    hash.u32(artifact.schema_version);
    hash.bytes(&artifact.episode_id.into_bytes());
    hash.bytes(&artifact.podcast_id.into_bytes());
    hash.text(&artifact.source_revision);
    hash_source(&mut hash, artifact.provenance.source);
    hash.optional_text(artifact.provenance.provider.as_deref());
    hash.bytes(&artifact.provenance.source_payload_digest.into_bytes());
    hash.text(&artifact.language);
    hash.i64(artifact.generated_at.value);
    hash.u64(artifact.speakers.len() as u64);
    for speaker in &artifact.speakers {
        hash.bytes(&speaker.speaker_id.into_bytes());
        hash.text(&speaker.label);
        hash.optional_text(speaker.display_name.as_deref());
    }
    hash.u64(artifact.segments.len() as u64);
    for segment in &artifact.segments {
        hash.u32(segment.ordinal);
        hash.text(&segment.text);
        hash.u64(segment.start_milliseconds);
        hash.u64(segment.end_milliseconds);
        hash.optional_id(segment.speaker_id.map(|id| id.into_bytes()));
        hash.u64(segment.words.len() as u64);
        for word in &segment.words {
            hash.text(&word.text);
            hash.u64(word.start_milliseconds);
            hash.u64(word.end_milliseconds);
        }
    }
    ContentDigest::from_bytes(hash.finish())
}

fn hash_source(hash: &mut StableHash, source: TranscriptSource) {
    match source {
        TranscriptSource::Publisher => hash.u32(1),
        TranscriptSource::Scribe => hash.u32(2),
        TranscriptSource::Whisper => hash.u32(3),
        TranscriptSource::OnDevice => hash.u32(4),
        TranscriptSource::AssemblyAi => hash.u32(5),
        TranscriptSource::Other => hash.u32(6),
        TranscriptSource::Unsupported { wire_code } => {
            hash.u32(u32::MAX);
            hash.u32(wire_code);
        }
    }
}
