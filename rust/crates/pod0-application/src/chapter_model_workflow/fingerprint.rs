use pod0_domain::{ChapterArtifact, ChapterArtifactSource, ContentDigest};
use sha2::{Digest as _, Sha256};

use crate::{ChapterModelObservationMode, ChapterModelResponseFormat, PlannedChapterModelRequest};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChapterModelRequestFingerprintError {
    InvalidEnrichmentBase,
}

pub fn chapter_model_request_fingerprint(
    request: &PlannedChapterModelRequest,
    configured_model: &str,
) -> Result<ContentDigest, ChapterModelRequestFingerprintError> {
    let mut hash = FramedHash::new();
    hash.text(&request.source_version);
    hash.bytes(&request.episode_id.into_bytes());
    hash.bytes(&request.podcast_id.into_bytes());
    hash.u32(request.format_version);
    hash.bytes(&request.requested_transcript_version_id.into_bytes());
    hash.bytes(&request.requested_transcript_content_digest.into_bytes());
    hash.bytes(&request.selected_transcript_version_id.into_bytes());
    hash.bytes(&request.selected_transcript_content_digest.into_bytes());
    hash.u32(request.policy_version);
    hash.text(&request.provider);
    hash.text(&request.model);
    hash.text(configured_model);
    hash.text(&request.system_prompt);
    hash.text(&request.user_prompt);
    match request.response_format {
        ChapterModelResponseFormat::JsonObject => hash.u32(0),
        ChapterModelResponseFormat::Unsupported { wire_code } => {
            hash.u32(1);
            hash.u32(wire_code);
        }
    }
    hash.u64(request.maximum_completion_bytes);
    match request.duration_milliseconds {
        Some(value) => {
            hash.u32(1);
            hash.u64(value);
        }
        None => hash.u32(0),
    }
    match &request.mode {
        ChapterModelObservationMode::Generate => hash.u32(0),
        ChapterModelObservationMode::Enrich { publisher_artifact } => {
            hash.u32(1);
            let base = ChapterArtifact::seal(publisher_artifact.clone())
                .map_err(|_| ChapterModelRequestFingerprintError::InvalidEnrichmentBase)?;
            hash.bytes(&base.artifact_id.into_bytes());
            hash.bytes(&base.integrity_digest.into_bytes());
        }
    }
    match request.expected_artifact_source {
        ChapterArtifactSource::Publisher => hash.u32(0),
        ChapterArtifactSource::Generated => hash.u32(1),
        ChapterArtifactSource::PublisherEnriched => hash.u32(2),
        ChapterArtifactSource::AgentComposed => hash.u32(3),
        ChapterArtifactSource::Unsupported { wire_code } => {
            hash.u32(4);
            hash.u32(wire_code);
        }
    }
    hash.u64(request.expected_chapter_selection_revision.value());
    Ok(hash.finish())
}

struct FramedHash(Sha256);

impl FramedHash {
    fn new() -> Self {
        let mut hash = Self(Sha256::new());
        hash.bytes(b"pod0.chapter-model-request.v1");
        hash
    }
    fn bytes(&mut self, value: &[u8]) {
        self.0.update((value.len() as u64).to_be_bytes());
        self.0.update(value);
    }
    fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }
    fn u32(&mut self, value: u32) {
        self.0.update(value.to_be_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.0.update(value.to_be_bytes());
    }
    fn finish(self) -> ContentDigest {
        ContentDigest::from_bytes(self.0.finalize().into())
    }
}
