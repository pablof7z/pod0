use crate::{
    ContentDigest, EpisodeId, PodcastId, SpeakerId, TranscriptArtifact, TranscriptArtifactError,
    TranscriptArtifactInput, TranscriptArtifactSegmentInput, TranscriptArtifactSpeakerInput,
    TranscriptArtifactWordInput, TranscriptSource, UnixTimestampMilliseconds,
};

fn input() -> TranscriptArtifactInput {
    let first = SpeakerId::from_parts(30, 1);
    let second = SpeakerId::from_parts(30, 2);
    TranscriptArtifactInput {
        episode_id: EpisodeId::from_parts(10, 1),
        podcast_id: PodcastId::from_parts(20, 1),
        source_revision: "selected-json-sha256:fixture-v1".to_owned(),
        source: TranscriptSource::Publisher,
        provider: Some("podcasting-2.0".to_owned()),
        source_payload_digest: ContentDigest::from_bytes([7; 32]),
        language: "en-US".to_owned(),
        generated_at: UnixTimestampMilliseconds::new(1_721_322_123_456),
        speakers: vec![
            TranscriptArtifactSpeakerInput {
                speaker_id: first,
                label: "spk_0".to_owned(),
                display_name: Some("Ada".to_owned()),
            },
            TranscriptArtifactSpeakerInput {
                speaker_id: second,
                label: "spk_1".to_owned(),
                display_name: None,
            },
        ],
        segments: vec![
            TranscriptArtifactSegmentInput {
                text: "  Calm   by default.  ".to_owned(),
                start_milliseconds: 1_001,
                end_milliseconds: 2_499,
                speaker_id: Some(first),
                words: vec![
                    word("Calm", 1_001, 1_500),
                    word("by", 1_501, 1_700),
                    word("default.", 1_701, 2_499),
                ],
            },
            TranscriptArtifactSegmentInput {
                text: "Alive on demand.".to_owned(),
                start_milliseconds: 2_300,
                end_milliseconds: 3_999,
                speaker_id: Some(second),
                words: vec![
                    word("Alive", 2_250, 2_800),
                    word("on", 2_801, 3_000),
                    word("demand.", 3_001, 4_050),
                ],
            },
        ],
    }
}

fn word(text: &str, start: u64, end: u64) -> TranscriptArtifactWordInput {
    TranscriptArtifactWordInput {
        text: text.to_owned(),
        start_milliseconds: start,
        end_milliseconds: end,
    }
}

#[test]
fn canonical_artifact_preserves_full_input_and_is_deterministic() {
    let source = input();
    let first = TranscriptArtifact::seal(source.clone()).expect("valid artifact");
    let second = TranscriptArtifact::seal(source).expect("same artifact");

    assert_eq!(first, second);
    assert_eq!(first.segments[0].text, "  Calm   by default.  ");
    assert_eq!(first.segments[1].start_milliseconds, 2_300);
    assert_eq!(first.segments[1].words[2].end_milliseconds, 4_050);
    assert_eq!(first.speakers[0].display_name.as_deref(), Some("Ada"));
    assert_eq!(
        first.artifact_id.into_bytes(),
        first.integrity_digest.into_bytes()[..16]
    );
    first.verify_integrity().expect("sealed artifact verifies");
}

#[test]
fn full_artifact_changes_do_not_retarget_semantic_evidence_identity() {
    let base = TranscriptArtifact::seal(input()).expect("base artifact");
    let mut revised = input();
    revised.speakers[0].display_name = Some("Dr. Ada".to_owned());
    revised.segments[0].words[0].end_milliseconds = 1_499;
    let revised = TranscriptArtifact::seal(revised).expect("revised artifact");

    assert_ne!(revised.artifact_id, base.artifact_id);
    assert_ne!(revised.integrity_digest, base.integrity_digest);
    assert_eq!(revised.transcript_version_id, base.transcript_version_id);
    assert_eq!(revised.content_digest, base.content_digest);
}

#[test]
fn exact_text_changes_update_both_artifact_and_semantic_version() {
    let base = TranscriptArtifact::seal(input()).expect("base artifact");
    let mut revised = input();
    revised.segments[0].text = "Calm by design.".to_owned();
    let revised = TranscriptArtifact::seal(revised).expect("revised artifact");

    assert_ne!(revised.artifact_id, base.artifact_id);
    assert_ne!(revised.transcript_version_id, base.transcript_version_id);
    assert_ne!(revised.content_digest, base.content_digest);
}

#[test]
fn unsupported_sources_round_trip_without_colliding_with_known_sources() {
    let known = TranscriptArtifact::seal(input()).expect("known artifact");
    let mut future = input();
    future.source = TranscriptSource::Unsupported { wire_code: 4_242 };
    let future = TranscriptArtifact::seal(future).expect("future source is preserved");

    assert_eq!(
        future.provenance.source,
        TranscriptSource::Unsupported { wire_code: 4_242 }
    );
    assert_ne!(future.artifact_id, known.artifact_id);
    assert_ne!(future.transcript_version_id, known.transcript_version_id);
}

#[test]
fn invalid_ranges_ordering_and_duplicate_speakers_fail_closed() {
    let mut invalid_segment = input();
    invalid_segment.segments[0].end_milliseconds = 1_000;
    assert_eq!(
        TranscriptArtifact::seal(invalid_segment),
        Err(TranscriptArtifactError::InvalidSegmentTime)
    );

    let mut invalid_words = input();
    invalid_words.segments[0].words[1].start_milliseconds = 900;
    assert_eq!(
        TranscriptArtifact::seal(invalid_words),
        Err(TranscriptArtifactError::WordsOutOfOrder)
    );

    let mut duplicate_speaker = input();
    duplicate_speaker.speakers[1].speaker_id = duplicate_speaker.speakers[0].speaker_id;
    assert_eq!(
        TranscriptArtifact::seal(duplicate_speaker),
        Err(TranscriptArtifactError::DuplicateSpeaker)
    );
}

#[test]
fn integrity_verification_detects_mutated_payloads() {
    let mut artifact = TranscriptArtifact::seal(input()).expect("artifact");
    artifact.segments[0].words[0].text = "Changed".to_owned();

    assert_eq!(
        artifact.verify_integrity(),
        Err(TranscriptArtifactError::IdentityMismatch)
    );
}
