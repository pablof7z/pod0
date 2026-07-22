use std::path::Path;

use pod0_domain::{CancellationId, CommandId, ContentDigest, StateRevision};

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::{
    TranscriptArtifactInput, TranscriptArtifactSegmentInput, TranscriptSource,
    UnixTimestampMilliseconds,
};

pub(super) fn install_empty_transcript_workflow_cutover(target: &Path) {
    let store = pod0_storage::LibraryStore::open_authoritative(target).unwrap();
    let source_fingerprint = pod0_storage::transcript_workflow_source_fingerprint(&[]);
    store
        .stage_legacy_transcript_workflow_cutover(
            pod0_storage::LegacyTranscriptWorkflowCutoverInput {
                source_generation: 1,
                source_fingerprint,
                backup_digest: ContentDigest::default(),
                backup_byte_count: 0,
                rows: Vec::new(),
                candidates: Vec::new(),
                command_id: CommandId::from_parts(9, 10),
                cancellation_id: CancellationId::from_parts(9, 11),
                issued_revision: StateRevision::INITIAL,
                max_attempts: pod0_application::TRANSCRIPT_WORKFLOW_MAX_ATTEMPTS,
                now_ms: 1_800_000_000_009,
            },
        )
        .unwrap();
    store
        .verify_legacy_transcript_workflow_cutover(1, source_fingerprint, 1_800_000_000_010)
        .unwrap();
    store
        .commit_legacy_transcript_workflow_cutover(1, source_fingerprint, 1_800_000_000_011)
        .unwrap();
}

pub(super) fn transcript_input(fixture: &PlaybackFixture) -> TranscriptArtifactInput {
    TranscriptArtifactInput {
        episode_id: fixture.episode_id,
        podcast_id: fixture.podcast_id,
        source_revision: "fixture-transcript-v1".to_owned(),
        source: TranscriptSource::Publisher,
        provider: Some("fixture".to_owned()),
        source_payload_digest: ContentDigest::from_bytes([0x45; 32]),
        language: "en-US".to_owned(),
        generated_at: UnixTimestampMilliseconds::new(1_800_000_000_009),
        speakers: Vec::new(),
        segments: vec![TranscriptArtifactSegmentInput {
            text: "Fixture transcript evidence".to_owned(),
            start_milliseconds: 0,
            end_milliseconds: 1_000,
            speaker_id: None,
            words: Vec::new(),
        }],
    }
}
