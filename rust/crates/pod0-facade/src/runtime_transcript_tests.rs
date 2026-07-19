use pod0_domain::{
    ContentDigest, SpeakerId, StateRevision, TranscriptArtifactInput,
    TranscriptArtifactSegmentInput, TranscriptArtifactSpeakerInput, TranscriptArtifactWordInput,
    TranscriptSource, UnixTimestampMilliseconds,
};

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn transcript_command_is_durable_replayable_and_page_bounded() {
    let fixture = PlaybackFixture::new();
    let command = envelope(1, StateRevision::INITIAL, input(&fixture, "source-v1"));
    fixture.facade.dispatch(command.clone());

    let summary = transcript(
        &fixture.facade,
        fixture.episode_id,
        TranscriptProjectionScope::Summary,
        0,
        20,
    );
    let selected = summary.summary.as_ref().expect("selected transcript");
    assert_eq!(selected.source_revision, "source-v1");
    assert_eq!(selected.selection_revision, StateRevision::new(1));
    assert_eq!(selected.segment_count, 2);
    assert!(summary.failure.is_none());
    let committed_operation = operation(&summary, 1);
    assert!(matches!(
        committed_operation.result,
        Some(OperationResult::TranscriptCommitted { receipt })
            if receipt.selection_revision == StateRevision::new(1)
                && receipt.segment_count == 2
    ));

    let segments = transcript(
        &fixture.facade,
        fixture.episode_id,
        TranscriptProjectionScope::Segments,
        0,
        1,
    );
    assert_eq!(segments.segments.len(), 1);
    assert!(segments.has_more);
    let segment_id = segments.segments[0].segment_id;
    let exact = transcript(
        &fixture.facade,
        fixture.episode_id,
        TranscriptProjectionScope::Segment { segment_id },
        0,
        20,
    );
    assert_eq!(exact.segments[0].text, "First mapped segment");
    let words = transcript(
        &fixture.facade,
        fixture.episode_id,
        TranscriptProjectionScope::Words { segment_id },
        0,
        1,
    );
    assert_eq!(words.words[0].text, "First");
    assert!(words.has_more);

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    assert_eq!(
        transcript(
            &reopened,
            fixture.episode_id,
            TranscriptProjectionScope::Summary,
            0,
            20,
        )
        .summary
        .expect("reopened selection")
        .transcript_content_digest,
        selected.transcript_content_digest
    );
    reopened.dispatch(command);
    assert!(matches!(
        operation(
            &transcript(
                &reopened,
                fixture.episode_id,
                TranscriptProjectionScope::Summary,
                0,
                20,
            ),
            1,
        )
        .result,
        Some(OperationResult::TranscriptCommitted { receipt })
            if receipt.selection_revision == StateRevision::new(1)
    ));
}

#[test]
fn stale_invalid_missing_and_unsupported_transcript_states_are_typed() {
    let fixture = PlaybackFixture::new();
    fixture.facade.dispatch(envelope(
        2,
        StateRevision::INITIAL,
        input(&fixture, "source-v1"),
    ));
    fixture.facade.dispatch(envelope(
        3,
        StateRevision::INITIAL,
        input(&fixture, "stale-source"),
    ));
    let projection = transcript(
        &fixture.facade,
        fixture.episode_id,
        TranscriptProjectionScope::Summary,
        0,
        20,
    );
    assert!(matches!(
        operation(&projection, 3).failure,
        Some(CoreFailure {
            code: CoreFailureCode::RevisionConflict,
            ..
        })
    ));

    let mut invalid = input(&fixture, "invalid-source");
    invalid.segments[0].end_milliseconds = 0;
    fixture
        .facade
        .dispatch(envelope(4, StateRevision::new(1), invalid));
    let invalid_projection = transcript(
        &fixture.facade,
        fixture.episode_id,
        TranscriptProjectionScope::Summary,
        0,
        20,
    );
    assert!(matches!(
        operation(&invalid_projection, 4).failure,
        Some(CoreFailure {
            code: CoreFailureCode::InvalidTranscript,
            ..
        })
    ));

    let missing = transcript(
        &fixture.facade,
        EpisodeId::from_parts(99, 99),
        TranscriptProjectionScope::Summary,
        0,
        20,
    );
    assert!(missing.summary.is_none() && missing.failure.is_none());
    let unsupported = transcript(
        &fixture.facade,
        fixture.episode_id,
        TranscriptProjectionScope::Unsupported { wire_code: 77 },
        0,
        20,
    );
    assert!(unsupported.summary.is_none());
    assert!(unsupported.segments.is_empty());
}

fn envelope(
    id: u64,
    expected_selection_revision: StateRevision,
    artifact: TranscriptArtifactInput,
) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(60, id),
        cancellation_id: CancellationId::from_parts(61, id),
        expected_revision: None,
        command: ApplicationCommand::CommitTranscript {
            expected_selection_revision,
            artifact,
        },
    }
}

fn input(fixture: &PlaybackFixture, source_revision: &str) -> TranscriptArtifactInput {
    let speaker_id = SpeakerId::from_parts(62, 1);
    TranscriptArtifactInput {
        episode_id: fixture.episode_id,
        podcast_id: fixture.podcast_id,
        source_revision: source_revision.to_owned(),
        source: TranscriptSource::AssemblyAi,
        provider: Some("assemblyAI".to_owned()),
        source_payload_digest: ContentDigest::from_bytes([0x55; 32]),
        language: "en-US".to_owned(),
        generated_at: UnixTimestampMilliseconds::new(1_800_000_000_100),
        speakers: vec![TranscriptArtifactSpeakerInput {
            speaker_id,
            label: "A".to_owned(),
            display_name: Some("Ada".to_owned()),
        }],
        segments: vec![
            segment(
                "First mapped segment",
                125,
                1_000,
                speaker_id,
                &["First", "mapped"],
            ),
            segment("Second mapped segment", 900, 2_000, speaker_id, &["Second"]),
        ],
    }
}

fn segment(
    text: &str,
    start: u64,
    end: u64,
    speaker_id: SpeakerId,
    words: &[&str],
) -> TranscriptArtifactSegmentInput {
    TranscriptArtifactSegmentInput {
        text: text.to_owned(),
        start_milliseconds: start,
        end_milliseconds: end,
        speaker_id: Some(speaker_id),
        words: words
            .iter()
            .enumerate()
            .map(|(index, text)| TranscriptArtifactWordInput {
                text: (*text).to_owned(),
                start_milliseconds: start + (index as u64 * 100),
                end_milliseconds: start + (index as u64 * 100) + 90,
            })
            .collect(),
    }
}

fn transcript(
    facade: &Pod0Facade,
    episode_id: EpisodeId,
    scope: TranscriptProjectionScope,
    offset: u32,
    max_items: u16,
) -> TranscriptProjection {
    let Projection::Transcript { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Transcript { episode_id, scope },
            offset,
            max_items,
        })
        .projection
    else {
        panic!("expected transcript projection");
    };
    value
}

fn operation(projection: &TranscriptProjection, id: u64) -> &OperationProjection {
    projection
        .operations
        .iter()
        .find(|operation| operation.command_id == CommandId::from_parts(60, id))
        .expect("transcript operation should be projected")
}
