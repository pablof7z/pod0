use crate::normalize_media_url;
use pod0_domain::{ContentDigest, EpisodeId, GeneratedArtifactId, PodcastId};
use sha2::{Digest as _, Sha256};

pub const MAX_AGENT_GENERATED_AUDIO_BYTES: u64 = 128 * 1_024 * 1_024;
pub const MAX_AGENT_GENERATED_AUDIO_DURATION_MILLISECONDS: u64 = 24 * 60 * 60 * 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AgentCapabilityExecutionMode {
    Perform,
    RecoverExisting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentGeneratedAudioTarget {
    pub artifact_id: GeneratedArtifactId,
    pub maximum_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AgentGeneratedAudioEvidence {
    pub artifact_id: GeneratedArtifactId,
    pub file_url: String,
    pub media_type: String,
    pub byte_count: u64,
    pub content_digest: ContentDigest,
    pub duration_milliseconds: Option<u64>,
}

#[must_use]
pub const fn default_agent_generated_podcast_id() -> PodcastId {
    PodcastId::from_bytes([
        0x6c, 0xb4, 0x5d, 0x63, 0x3c, 0x88, 0x5f, 0xb5, 0xa7, 0xd3, 0x27, 0x0d, 0xb6, 0x0e, 0xf5,
        0x14,
    ])
}

#[must_use]
pub fn agent_generated_episode_id(podcast_id: PodcastId, audio_url: &str) -> EpisodeId {
    let mut hash = Sha256::new();
    hash.update(podcast_id.into_bytes());
    hash.update(audio_url.as_bytes());
    EpisodeId::from_bytes(hash.finalize()[..16].try_into().expect("digest slice"))
}

#[must_use]
pub fn agent_generated_audio_evidence_is_valid(
    evidence: &AgentGeneratedAudioEvidence,
    target: AgentGeneratedAudioTarget,
) -> bool {
    evidence.artifact_id == target.artifact_id
        && target.maximum_bytes == MAX_AGENT_GENERATED_AUDIO_BYTES
        && (1..=target.maximum_bytes).contains(&evidence.byte_count)
        && evidence.media_type == "audio/mpeg"
        && evidence.file_url.starts_with("file://")
        && normalize_media_url(&evidence.file_url).as_deref() == Some(evidence.file_url.as_str())
        && evidence.duration_milliseconds.is_none_or(|duration| {
            (1..=MAX_AGENT_GENERATED_AUDIO_DURATION_MILLISECONDS).contains(&duration)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence() -> AgentGeneratedAudioEvidence {
        AgentGeneratedAudioEvidence {
            artifact_id: GeneratedArtifactId::from_parts(1, 2),
            file_url: "file:///private/agent/brief.mp3".into(),
            media_type: "audio/mpeg".into(),
            byte_count: 1_024,
            content_digest: ContentDigest::from_bytes([3; 32]),
            duration_milliseconds: Some(12_000),
        }
    }

    #[test]
    fn evidence_requires_the_exact_bounded_local_artifact() {
        let target = AgentGeneratedAudioTarget {
            artifact_id: GeneratedArtifactId::from_parts(1, 2),
            maximum_bytes: MAX_AGENT_GENERATED_AUDIO_BYTES,
        };
        assert!(agent_generated_audio_evidence_is_valid(&evidence(), target));
        let mut remote = evidence();
        remote.file_url = "https://example.test/brief.mp3".into();
        assert!(!agent_generated_audio_evidence_is_valid(&remote, target));
        let mut oversized = evidence();
        oversized.byte_count = MAX_AGENT_GENERATED_AUDIO_BYTES + 1;
        assert!(!agent_generated_audio_evidence_is_valid(&oversized, target));
    }
}
