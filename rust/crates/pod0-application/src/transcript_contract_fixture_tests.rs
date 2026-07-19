use std::collections::BTreeMap;

use pod0_domain::{
    CommandId, ContentDigest, EpisodeId, PodcastId, SpeakerId, StateRevision, TranscriptArtifact,
    TranscriptArtifactId, TranscriptArtifactInput, TranscriptArtifactSegmentInput,
    TranscriptArtifactSpeakerInput, TranscriptArtifactWordInput, TranscriptSource,
    TranscriptVersionId, UnixTimestampMilliseconds,
};

use crate::{
    FACADE_CONTRACT_VERSION, TranscriptCommitRequest, TranscriptProjectionScope,
    project_transcript_artifact, qualify_transcript_commit,
};

fn values() -> BTreeMap<&'static str, &'static str> {
    include_str!("../../../../Fixtures/CoreKnowledge/transcript-contract-v1.properties")
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

fn digest(values: &BTreeMap<&str, &str>, prefix: &str) -> ContentDigest {
    ContentDigest {
        word_0: number(values, &format!("{prefix}_word_0")),
        word_1: number(values, &format!("{prefix}_word_1")),
        word_2: number(values, &format!("{prefix}_word_2")),
        word_3: number(values, &format!("{prefix}_word_3")),
    }
}

fn fixture_request(values: &BTreeMap<&str, &str>) -> TranscriptCommitRequest {
    let speakers = (0..number(values, "speaker_count"))
        .map(|index| {
            let prefix = format!("speaker_{index}");
            let (high, low) = id(values, &format!("{prefix}_id"));
            TranscriptArtifactSpeakerInput {
                speaker_id: SpeakerId::from_parts(high, low),
                label: values[format!("{prefix}_label").as_str()].to_owned(),
                display_name: Some(values[format!("{prefix}_display_name").as_str()].to_owned()),
            }
        })
        .collect::<Vec<_>>();
    let segments = (0..number(values, "segment_count"))
        .map(|index| {
            let prefix = format!("segment_{index}");
            let speaker_index: usize = number(values, &format!("{prefix}_speaker_index"));
            let words = (0..number(values, &format!("{prefix}_word_count")))
                .map(|word_index| {
                    let word = format!("{prefix}_word_{word_index}");
                    TranscriptArtifactWordInput {
                        text: values[format!("{word}_text").as_str()].to_owned(),
                        start_milliseconds: number(values, &format!("{word}_start_milliseconds")),
                        end_milliseconds: number(values, &format!("{word}_end_milliseconds")),
                    }
                })
                .collect();
            TranscriptArtifactSegmentInput {
                text: values[format!("{prefix}_text").as_str()].to_owned(),
                start_milliseconds: number(values, &format!("{prefix}_start_milliseconds")),
                end_milliseconds: number(values, &format!("{prefix}_end_milliseconds")),
                speaker_id: Some(speakers[speaker_index].speaker_id),
                words,
            }
        })
        .collect();
    let (command_high, command_low) = id(values, "command_id");
    let (episode_high, episode_low) = id(values, "episode_id");
    let (podcast_high, podcast_low) = id(values, "podcast_id");
    assert_eq!(values["source"], "unsupported");
    TranscriptCommitRequest {
        command_id: CommandId::from_parts(command_high, command_low),
        expected_selection_revision: StateRevision::new(number(
            values,
            "expected_selection_revision",
        )),
        artifact: TranscriptArtifactInput {
            episode_id: EpisodeId::from_parts(episode_high, episode_low),
            podcast_id: PodcastId::from_parts(podcast_high, podcast_low),
            source_revision: values["source_revision"].to_owned(),
            source: TranscriptSource::Unsupported {
                wire_code: number(values, "source_wire_code"),
            },
            provider: Some(values["provider"].to_owned()),
            source_payload_digest: digest(values, "source_payload_digest"),
            language: values["language"].to_owned(),
            generated_at: UnixTimestampMilliseconds::new(number(
                values,
                "generated_at_milliseconds",
            )),
            speakers,
            segments,
        },
    }
}

#[test]
fn rust_qualifies_the_cross_platform_transcript_fixture() {
    let values = values();
    assert_eq!(number::<u32>(&values, "fixture_version"), 1);
    assert_eq!(
        number::<u32>(&values, "contract_version"),
        FACADE_CONTRACT_VERSION
    );
    assert_eq!(values["unknown_future_field"], "ignored-by-v1-readers");
    let request = fixture_request(&values);
    let artifact = TranscriptArtifact::seal(request.artifact.clone()).expect("artifact");
    let receipt = qualify_transcript_commit(request).expect("receipt");
    let (artifact_high, artifact_low) = id(&values, "expected_artifact_id");
    let (version_high, version_low) = id(&values, "expected_transcript_version_id");

    assert_eq!(
        receipt.artifact_id,
        TranscriptArtifactId::from_parts(artifact_high, artifact_low)
    );
    assert_eq!(
        receipt.transcript_version_id,
        TranscriptVersionId::from_parts(version_high, version_low)
    );
    assert_eq!(
        receipt.transcript_content_digest,
        digest(&values, "expected_content_digest")
    );
    assert_eq!(
        receipt.artifact_integrity_digest,
        digest(&values, "expected_integrity_digest")
    );
    assert_eq!(
        receipt.command_fingerprint,
        digest(&values, "expected_command_fingerprint")
    );
    assert_eq!(
        receipt.selection_revision,
        StateRevision::new(number(&values, "expected_committed_selection_revision"))
    );
    assert_eq!(
        receipt.speaker_count,
        number::<u32>(&values, "speaker_count")
    );
    assert_eq!(
        receipt.segment_count,
        number::<u32>(&values, "segment_count")
    );
    assert_eq!(
        receipt.word_count,
        number::<u64>(&values, "expected_word_count")
    );
    for (index, segment) in artifact.segments.iter().enumerate() {
        let (high, low) = id(&values, &format!("expected_segment_{index}_id"));
        assert_eq!(
            segment.segment_id,
            pod0_domain::TranscriptSegmentId::from_parts(high, low)
        );
    }

    let speakers = project_transcript_artifact(
        &artifact,
        receipt.selection_revision,
        TranscriptProjectionScope::Speakers,
    );
    let words = project_transcript_artifact(
        &artifact,
        receipt.selection_revision,
        TranscriptProjectionScope::Words {
            segment_id: artifact.segments[1].segment_id,
        },
    );
    assert_eq!(speakers.speakers[1].display_name.as_deref(), Some("Grace"));
    assert_eq!(words.words[2].end_milliseconds, 4_050);
    assert!(matches!(
        speakers.summary.expect("summary").source,
        TranscriptSource::Unsupported { wire_code: 4_242 }
    ));
}
