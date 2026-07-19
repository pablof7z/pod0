use std::collections::BTreeMap;

use pod0_domain::{
    ContentDigest, EpisodeId, EvidenceSpanId, PodcastId, SpeakerId, TranscriptSource,
    TranscriptVersionId,
};

use crate::{EvidenceChunkPolicy, TranscriptEvidenceInput, TranscriptSegmentInput};

pub(crate) struct GoldenEvidenceFixture {
    pub input: TranscriptEvidenceInput,
    pub policy: EvidenceChunkPolicy,
    pub expected_version_id: TranscriptVersionId,
    pub expected_content_digest: ContentDigest,
    pub expected_span_id: EvidenceSpanId,
    pub expected_span_count: usize,
    pub expected_span_start_milliseconds: u64,
    pub expected_span_end_milliseconds: u64,
    pub expected_span_text: String,
}

fn values() -> BTreeMap<&'static str, &'static str> {
    include_str!("../../../../Fixtures/CoreKnowledge/transcript-evidence-v1.properties")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split_once('=').expect("valid golden property"))
        .collect()
}

fn number<T: std::str::FromStr>(values: &BTreeMap<&str, &str>, key: &str) -> T {
    values[key]
        .parse()
        .unwrap_or_else(|_| panic!("valid {key}"))
}

fn id(values: &BTreeMap<&str, &str>, prefix: &str) -> (u64, u64) {
    (
        number(values, &format!("{prefix}_high")),
        number(values, &format!("{prefix}_low")),
    )
}

fn content_digest(values: &BTreeMap<&str, &str>, prefix: &str) -> ContentDigest {
    ContentDigest {
        word_0: number(values, &format!("{prefix}_word_0")),
        word_1: number(values, &format!("{prefix}_word_1")),
        word_2: number(values, &format!("{prefix}_word_2")),
        word_3: number(values, &format!("{prefix}_word_3")),
    }
}

pub(crate) fn digest(byte: u8) -> ContentDigest {
    ContentDigest::from_bytes([byte; 32])
}

pub(crate) fn segment(
    text: impl Into<String>,
    start_milliseconds: u64,
    end_milliseconds: u64,
    speaker_id: Option<SpeakerId>,
) -> TranscriptSegmentInput {
    TranscriptSegmentInput {
        text: text.into(),
        start_milliseconds,
        end_milliseconds,
        speaker_id,
    }
}

pub(crate) fn golden_fixture() -> GoldenEvidenceFixture {
    let values = values();
    assert_eq!(number::<u32>(&values, "fixture_version"), 1);
    assert_eq!(values["unknown_future_field"], "ignored-by-v1-readers");
    let (episode_high, episode_low) = id(&values, "episode_id");
    let (podcast_high, podcast_low) = id(&values, "podcast_id");
    let source = match values["source"] {
        "publisher" => TranscriptSource::Publisher,
        value => panic!("unsupported fixture transcript source: {value}"),
    };
    let segment_count = number(&values, "segment_count");
    let segments = (0..segment_count)
        .map(|index| {
            let prefix = format!("segment_{index}");
            let (speaker_high, speaker_low) = id(&values, &format!("{prefix}_speaker_id"));
            segment(
                values[format!("{prefix}_text").as_str()],
                number(&values, &format!("{prefix}_start_ms")),
                number(&values, &format!("{prefix}_end_ms")),
                Some(SpeakerId::from_parts(speaker_high, speaker_low)),
            )
        })
        .collect();
    let (version_high, version_low) = id(&values, "expected_transcript_version_id");
    let (span_high, span_low) = id(&values, "expected_span_id");

    GoldenEvidenceFixture {
        input: TranscriptEvidenceInput {
            episode_id: EpisodeId::from_parts(episode_high, episode_low),
            podcast_id: PodcastId::from_parts(podcast_high, podcast_low),
            source_revision: values["source_revision"].to_owned(),
            source,
            provider: Some(values["provider"].to_owned()),
            source_payload_digest: content_digest(&values, "source_payload_digest"),
            segments,
        },
        policy: EvidenceChunkPolicy {
            version: number(&values, "chunk_policy_version"),
            target_tokens: number(&values, "chunk_target_tokens"),
            overlap_per_mille: number(&values, "chunk_overlap_per_mille"),
            snap_tolerance_per_mille: number(&values, "chunk_snap_tolerance_per_mille"),
        },
        expected_version_id: TranscriptVersionId::from_parts(version_high, version_low),
        expected_content_digest: content_digest(&values, "expected_content_digest"),
        expected_span_id: EvidenceSpanId::from_parts(span_high, span_low),
        expected_span_count: number(&values, "expected_span_count"),
        expected_span_start_milliseconds: number(&values, "expected_span_start_ms"),
        expected_span_end_milliseconds: number(&values, "expected_span_end_ms"),
        expected_span_text: values["expected_span_text"].to_owned(),
    }
}

pub(crate) fn golden_input() -> TranscriptEvidenceInput {
    golden_fixture().input
}
