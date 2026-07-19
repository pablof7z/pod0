use pod0_domain::SpeakerId;

use crate::knowledge_test_fixture::{digest, golden_fixture, golden_input, segment};
use crate::{
    EvidenceBuildError, EvidenceChunkPolicy, MAX_PROVENANCE_PROVIDER_BYTES, MAX_SEGMENT_TEXT_BYTES,
    build_evidence_artifact,
};

#[test]
fn golden_span_is_replayable_and_preserves_the_playable_anchor() {
    let fixture = golden_fixture();
    let first = build_evidence_artifact(&fixture.input, fixture.policy).unwrap();
    let second = build_evidence_artifact(&fixture.input, fixture.policy).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.segments.len(), 2);
    assert_eq!(first.spans.len(), fixture.expected_span_count);
    let span = &first.spans[0];
    assert_eq!(
        span.start_milliseconds,
        fixture.expected_span_start_milliseconds
    );
    assert_eq!(
        span.end_milliseconds,
        fixture.expected_span_end_milliseconds
    );
    assert_eq!(span.text, fixture.expected_span_text);
    assert_eq!(span.first_segment_id, first.segments[0].segment_id);
    assert_eq!(span.last_segment_id, first.segments[1].segment_id);
    assert_eq!(span.provenance, first.version.provenance);
    assert_eq!(
        first.version.transcript_version_id,
        fixture.expected_version_id
    );
    assert_eq!(span.span_id, fixture.expected_span_id);
    assert_eq!(
        first.version.content_digest,
        fixture.expected_content_digest
    );
}

#[test]
fn every_identity_bearing_input_changes_the_affected_identity() {
    let baseline =
        build_evidence_artifact(&golden_input(), EvidenceChunkPolicy::default()).unwrap();
    let mut changed_revision = golden_input();
    changed_revision.source_revision = "transcript-v4".to_owned();
    let revision =
        build_evidence_artifact(&changed_revision, EvidenceChunkPolicy::default()).unwrap();
    assert_ne!(
        revision.version.transcript_version_id,
        baseline.version.transcript_version_id
    );
    assert_ne!(revision.spans[0].span_id, baseline.spans[0].span_id);

    let mut changed_payload = golden_input();
    changed_payload.source_payload_digest = digest(0x56);
    let payload =
        build_evidence_artifact(&changed_payload, EvidenceChunkPolicy::default()).unwrap();
    assert_ne!(
        payload.version.transcript_version_id,
        baseline.version.transcript_version_id
    );

    let mut changed_text = golden_input();
    changed_text.segments[0].text.push_str(" today");
    let text = build_evidence_artifact(&changed_text, EvidenceChunkPolicy::default()).unwrap();
    assert_ne!(text.version.content_digest, baseline.version.content_digest);
    assert_ne!(text.segments[0].segment_id, baseline.segments[0].segment_id);

    let mut changed_time = golden_input();
    changed_time.segments[0].start_milliseconds += 1;
    let time = build_evidence_artifact(&changed_time, EvidenceChunkPolicy::default()).unwrap();
    assert_ne!(time.version.content_digest, baseline.version.content_digest);
    assert_ne!(time.spans[0].span_id, baseline.spans[0].span_id);

    let changed_policy = build_evidence_artifact(
        &golden_input(),
        EvidenceChunkPolicy {
            version: 2,
            ..EvidenceChunkPolicy::default()
        },
    )
    .unwrap();
    assert_eq!(changed_policy.version, baseline.version);
    assert_eq!(changed_policy.segments, baseline.segments);
    assert_ne!(changed_policy.spans[0].span_id, baseline.spans[0].span_id);
}

#[test]
fn empty_single_oversized_and_overlapping_segments_are_explicit() {
    let mut empty = golden_input();
    empty.segments = vec![segment(" \n ", 0, 0, None)];
    let empty = build_evidence_artifact(&empty, EvidenceChunkPolicy::default()).unwrap();
    assert!(empty.segments.is_empty());
    assert!(empty.spans.is_empty());

    let mut single = golden_input();
    single.segments = vec![segment("one grounded utterance", 10, 20, None)];
    let single = build_evidence_artifact(&single, EvidenceChunkPolicy::default()).unwrap();
    assert_eq!(single.spans.len(), 1);

    let mut oversized = golden_input();
    oversized.segments = vec![segment("word ".repeat(500), 10, 20, None)];
    let oversized = build_evidence_artifact(
        &oversized,
        EvidenceChunkPolicy {
            target_tokens: 20,
            ..EvidenceChunkPolicy::default()
        },
    )
    .unwrap();
    assert_eq!(oversized.spans.len(), 1);

    let mut overlapping = golden_input();
    overlapping.segments = vec![
        segment("first speaker remains audible", 0, 10_000, None),
        segment("second starts before first ends", 8_000, 9_000, None),
    ];
    let overlapping =
        build_evidence_artifact(&overlapping, EvidenceChunkPolicy::default()).unwrap();
    assert_eq!(overlapping.spans[0].end_milliseconds, 10_000);
}

#[test]
fn overlap_and_speaker_snapping_produce_bounded_ordered_spans() {
    let alice = SpeakerId::from_parts(0, 1);
    let bob = SpeakerId::from_parts(0, 2);
    let mut input = golden_input();
    input.segments = (0_u64..80)
        .map(|index| {
            let speaker = if index < 40 { alice } else { bob };
            segment(
                format!("segment {index} has enough words for stable token counting"),
                index * 1_000,
                (index + 1) * 1_000,
                Some(speaker),
            )
        })
        .collect();
    let artifact = build_evidence_artifact(
        &input,
        EvidenceChunkPolicy {
            target_tokens: 40,
            ..EvidenceChunkPolicy::default()
        },
    )
    .unwrap();
    assert!(artifact.spans.len() > 2);
    for pair in artifact.spans.windows(2) {
        assert!(pair[0].start_milliseconds < pair[1].start_milliseconds);
        assert_ne!(pair[0].text, pair[1].text);
    }
    assert!(artifact.spans.iter().all(|span| span.text.len() <= 65_536));
}

#[test]
fn speaker_boundary_snap_prefers_the_nearest_turn() {
    let alice = SpeakerId::from_parts(0, 1);
    let bob = SpeakerId::from_parts(0, 2);
    let mut input = golden_input();
    input.segments = (0_u64..6)
        .map(|index| {
            segment(
                "one two three four five",
                index * 1_000,
                (index + 1) * 1_000,
                Some(if index < 3 { alice } else { bob }),
            )
        })
        .collect();
    let artifact = build_evidence_artifact(
        &input,
        EvidenceChunkPolicy {
            target_tokens: 20,
            snap_tolerance_per_mille: 500,
            ..EvidenceChunkPolicy::default()
        },
    )
    .unwrap();
    assert_eq!(artifact.spans[0].end_segment_ordinal_exclusive, 3);
    assert_eq!(artifact.spans[0].speaker_id, Some(alice));
}

#[test]
fn dominant_speaker_ties_use_the_smallest_stable_identity() {
    let smaller = SpeakerId::from_parts(0, 1);
    let larger = SpeakerId::from_parts(0, 2);
    let mut input = golden_input();
    input.segments = vec![
        segment("same token count", 0, 1_000, Some(larger)),
        segment("same token count", 1_000, 2_000, Some(smaller)),
    ];
    let artifact = build_evidence_artifact(&input, EvidenceChunkPolicy::default()).unwrap();
    assert_eq!(artifact.spans[0].speaker_id, Some(smaller));
}

#[test]
fn invalid_and_unbounded_inputs_fail_closed() {
    let mut input = golden_input();
    input.source_revision = "   ".to_owned();
    assert_eq!(
        build_evidence_artifact(&input, EvidenceChunkPolicy::default()),
        Err(EvidenceBuildError::EmptySourceRevision)
    );

    let mut input = golden_input();
    input.provider = Some("p".repeat(MAX_PROVENANCE_PROVIDER_BYTES + 1));
    assert_eq!(
        build_evidence_artifact(&input, EvidenceChunkPolicy::default()),
        Err(EvidenceBuildError::ProviderTooLong)
    );

    let mut input = golden_input();
    input.segments[0].text = "x".repeat(MAX_SEGMENT_TEXT_BYTES + 1);
    assert_eq!(
        build_evidence_artifact(&input, EvidenceChunkPolicy::default()),
        Err(EvidenceBuildError::SegmentTextTooLong { ordinal: 0 })
    );

    let mut input = golden_input();
    input.segments[1].start_milliseconds = 1;
    assert_eq!(
        build_evidence_artifact(&input, EvidenceChunkPolicy::default()),
        Err(EvidenceBuildError::SegmentsOutOfOrder { ordinal: 1 })
    );

    let mut input = golden_input();
    input.segments[0].end_milliseconds = input.segments[0].start_milliseconds - 1;
    assert_eq!(
        build_evidence_artifact(&input, EvidenceChunkPolicy::default()),
        Err(EvidenceBuildError::InvalidSegmentTime { ordinal: 0 })
    );

    let mut input = golden_input();
    input.segments = vec![input.segments[0].clone(); crate::MAX_TRANSCRIPT_SEGMENTS + 1];
    assert_eq!(
        build_evidence_artifact(&input, EvidenceChunkPolicy::default()),
        Err(EvidenceBuildError::TooManySegments)
    );

    assert_eq!(
        build_evidence_artifact(
            &golden_input(),
            EvidenceChunkPolicy {
                overlap_per_mille: 501,
                ..EvidenceChunkPolicy::default()
            }
        ),
        Err(EvidenceBuildError::InvalidPolicy)
    );
}
