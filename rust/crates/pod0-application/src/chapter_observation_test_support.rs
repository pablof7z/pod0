use pod0_domain::{
    ChapterArtifact, ContentDigest, EpisodeId, PodcastId, TranscriptVersionId,
    UnixTimestampMilliseconds,
};

use crate::chapter_observation_values::payload_digest;
use crate::{
    AgentComposedChapterItem, AgentComposedChapterObservation, CHAPTER_MODEL_FORMAT_VERSION,
    ChapterModelObservationMode, ChapterObservationProjection, ModelChapterObservation,
    PublisherChapterObservation,
};

pub(crate) fn digest(value: &[u8]) -> ContentDigest {
    payload_digest(value)
}

pub(crate) fn publisher(payload: &str) -> PublisherChapterObservation {
    PublisherChapterObservation {
        episode_id: EpisodeId::from_parts(11, 101),
        podcast_id: PodcastId::from_parts(12, 101),
        resolved_source_url: "https://example.test/chapters.json".into(),
        content_type: "application/json; charset=utf-8".into(),
        payload_digest: digest(payload.as_bytes()),
        payload: payload.as_bytes().to_vec(),
        generated_at: UnixTimestampMilliseconds::new(1_721_322_123_456),
        duration_milliseconds: Some(120_000),
    }
}

pub(crate) fn model(
    completion: &str,
    mode: ChapterModelObservationMode,
) -> ModelChapterObservation {
    let transcript_digest = digest(b"selected transcript");
    ModelChapterObservation {
        episode_id: EpisodeId::from_parts(11, 101),
        podcast_id: PodcastId::from_parts(12, 101),
        format_version: CHAPTER_MODEL_FORMAT_VERSION,
        requested_transcript_version_id: TranscriptVersionId::from_parts(13, 101),
        requested_transcript_content_digest: transcript_digest,
        selected_transcript_version_id: TranscriptVersionId::from_parts(13, 101),
        selected_transcript_content_digest: transcript_digest,
        policy_version: 1,
        source_version: "model-input-v1".into(),
        provider: "openrouter".into(),
        model: "fixture-model-v1".into(),
        completion_digest: digest(completion.as_bytes()),
        completion: completion.into(),
        generated_at: UnixTimestampMilliseconds::new(1_721_322_123_456),
        duration_milliseconds: Some(120_000),
        mode,
    }
}

pub(crate) fn agent() -> AgentComposedChapterObservation {
    AgentComposedChapterObservation {
        episode_id: EpisodeId::from_parts(11, 101),
        podcast_id: PodcastId::from_parts(12, 101),
        composition_revision: "agent-composition-v1".into(),
        policy_version: 1,
        provider: Some("elevenlabs".into()),
        model: Some("eleven-multilingual-v2".into()),
        source_payload_digest: digest(b"ordered source turns"),
        generated_at: UnixTimestampMilliseconds::new(1_721_322_123_456),
        duration_milliseconds: Some(30_000),
        items: vec![
            AgentComposedChapterItem {
                start_seconds: 0.0,
                end_seconds: 10.25,
                title: "Opening synthesis".into(),
                summary: Some("  Calm   context.  ".into()),
                image_url: Some("file:///tmp/agent-art.jpg".into()),
                link_url: None,
                include_in_table_of_contents: true,
                source_episode_id: None,
            },
            AgentComposedChapterItem {
                start_seconds: 10.25,
                end_seconds: 30.0,
                title: "Source moment".into(),
                summary: None,
                image_url: Some("https://example.test/source.jpg".into()),
                link_url: Some("https://example.test/episode?t=42".into()),
                include_in_table_of_contents: true,
                source_episode_id: Some(EpisodeId::from_parts(99, 7)),
            },
        ],
    }
}

pub(crate) fn qualified(
    projection: ChapterObservationProjection,
) -> (ChapterArtifact, ContentDigest) {
    let ChapterObservationProjection::Qualified {
        artifact,
        observation_fingerprint,
    } = projection
    else {
        panic!("expected qualified observation, got {projection:?}");
    };
    (
        ChapterArtifact::seal(artifact).expect("qualified artifact must seal"),
        observation_fingerprint,
    )
}
