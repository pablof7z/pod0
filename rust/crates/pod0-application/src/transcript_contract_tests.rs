use pod0_domain::{
    CommandId, ContentDigest, EpisodeId, PodcastId, SpeakerId, StateRevision, TranscriptArtifact,
    TranscriptArtifactInput, TranscriptArtifactSegmentInput, TranscriptArtifactSpeakerInput,
    TranscriptArtifactWordInput, TranscriptSource, UnixTimestampMilliseconds,
};

use crate::{
    TranscriptCommitRequest, TranscriptContractError, TranscriptContractProjection,
    TranscriptContractRejection, TranscriptProjectionScope, project_transcript_artifact,
    project_transcript_contract, qualify_transcript_commit,
};

fn artifact_input() -> TranscriptArtifactInput {
    let speaker_id = SpeakerId::from_parts(3, 1);
    TranscriptArtifactInput {
        episode_id: EpisodeId::from_parts(1, 1),
        podcast_id: PodcastId::from_parts(2, 1),
        source_revision: "fixture-revision".to_owned(),
        source: TranscriptSource::AssemblyAi,
        provider: Some("assemblyai".to_owned()),
        source_payload_digest: ContentDigest::from_bytes([9; 32]),
        language: "en-US".to_owned(),
        generated_at: UnixTimestampMilliseconds::new(1_721_000_000_123),
        speakers: vec![TranscriptArtifactSpeakerInput {
            speaker_id,
            label: "A".to_owned(),
            display_name: Some("Ada".to_owned()),
        }],
        segments: (0_u64..3)
            .map(|index| TranscriptArtifactSegmentInput {
                text: format!("Segment {index}"),
                start_milliseconds: index * 1_000 + 1,
                end_milliseconds: index * 1_000 + 900,
                speaker_id: Some(speaker_id),
                words: vec![TranscriptArtifactWordInput {
                    text: format!("word{index}"),
                    start_milliseconds: index * 1_000 + 1,
                    end_milliseconds: index * 1_000 + 400,
                }],
            })
            .collect(),
    }
}

fn request(revision: u64) -> TranscriptCommitRequest {
    TranscriptCommitRequest {
        command_id: CommandId::from_parts(4, 1),
        expected_selection_revision: StateRevision::new(revision),
        artifact: artifact_input(),
    }
}

#[test]
fn qualification_is_deterministic_and_revision_sensitive() {
    let first = qualify_transcript_commit(request(7)).expect("qualified");
    let replay = qualify_transcript_commit(request(7)).expect("same receipt");
    let next = qualify_transcript_commit(request(8)).expect("next revision");

    assert_eq!(first, replay);
    assert_eq!(first.selection_revision, StateRevision::new(8));
    assert_eq!(first.speaker_count, 1);
    assert_eq!(first.segment_count, 3);
    assert_eq!(first.word_count, 3);
    assert_eq!(first.artifact_id, next.artifact_id);
    assert_ne!(first.command_fingerprint, next.command_fingerprint);
    assert_eq!(next.selection_revision, StateRevision::new(9));
}

#[test]
fn revision_overflow_and_invalid_payloads_are_typed() {
    assert!(matches!(
        qualify_transcript_commit(request(u64::MAX)),
        Err(TranscriptContractError::RevisionExhausted)
    ));

    let mut invalid = request(0);
    invalid.artifact.segments[0].words[0].end_milliseconds = 0;
    assert!(matches!(
        qualify_transcript_commit(invalid),
        Err(TranscriptContractError::InvalidWord)
    ));
}

#[test]
fn ffi_contract_rejections_are_projection_state_not_errors() {
    let mut invalid = request(0);
    invalid.artifact.segments[0].words[0].end_milliseconds = 0;
    assert_eq!(
        project_transcript_contract(invalid, TranscriptProjectionScope::Summary, 0, 20,),
        TranscriptContractProjection::Rejected {
            reason: TranscriptContractRejection::InvalidWord,
        }
    );
}

#[test]
fn transcript_projection_pages_each_collection_independently() {
    let artifact = TranscriptArtifact::seal(artifact_input()).expect("artifact");
    let mut segments = project_transcript_artifact(
        &artifact,
        StateRevision::new(5),
        TranscriptProjectionScope::Segments,
    );
    segments.enforce_bounds(1, 1);
    assert_eq!(
        segments.summary.as_ref().map(|value| value.segment_count),
        Some(3)
    );
    assert_eq!(segments.segments.len(), 1);
    assert_eq!(segments.segments[0].ordinal, 1);
    assert!(segments.has_more);

    let segment_id = artifact.segments[2].segment_id;
    let mut words = project_transcript_artifact(
        &artifact,
        StateRevision::new(5),
        TranscriptProjectionScope::Words { segment_id },
    );
    words.enforce_bounds(0, 20);
    assert_eq!(words.words.len(), 1);
    assert_eq!(words.words[0].segment_id, segment_id);

    let mut unsupported = project_transcript_artifact(
        &artifact,
        StateRevision::new(5),
        TranscriptProjectionScope::Unsupported { wire_code: 42 },
    );
    unsupported.enforce_bounds(0, 20);
    assert!(unsupported.summary.is_none());
    assert!(unsupported.segments.is_empty());
}
