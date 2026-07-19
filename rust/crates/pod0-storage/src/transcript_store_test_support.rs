use pod0_application::{
    EvidenceChunkPolicy, TranscriptEvidenceInput, TranscriptSegmentInput, build_evidence_artifact,
};
use pod0_domain::{
    CommandId, ContentDigest, EpisodeId, PodcastId, SpeakerId, TranscriptArtifactInput,
    TranscriptArtifactSegmentInput, TranscriptArtifactSpeakerInput, TranscriptArtifactWordInput,
    TranscriptEvidenceArtifact, TranscriptSource, UnixTimestampMilliseconds,
};

use crate::TranscriptStore;
use crate::listening_import_test_support::{
    EPISODE_ID, ImportFixture, create_sqlite_source, current_metadata, episode,
};

pub(crate) struct TranscriptFixture {
    pub(crate) import: ImportFixture,
    pub(crate) store: TranscriptStore,
}

impl TranscriptFixture {
    pub(crate) fn new() -> Self {
        let import = ImportFixture::new();
        create_sqlite_source(
            &import.source,
            &current_metadata(12),
            &[episode(EPISODE_ID, "transcript-guid")],
        );
        import.stage(&import.plan()).unwrap();
        let store = TranscriptStore::open(&import.target).unwrap();
        Self { import, store }
    }
}

pub(crate) fn command(value: u64) -> CommandId {
    CommandId::from_parts(0, value)
}

pub(crate) fn input(revision: &str) -> TranscriptArtifactInput {
    let first_speaker = SpeakerId::from_bytes([0x44; 16]);
    let second_speaker = SpeakerId::from_bytes([0x45; 16]);
    TranscriptArtifactInput {
        episode_id: EpisodeId::from_bytes([0x22; 16]),
        podcast_id: PodcastId::from_bytes([0x11; 16]),
        source_revision: revision.to_owned(),
        source: TranscriptSource::Publisher,
        provider: Some("publisher-feed".to_owned()),
        source_payload_digest: ContentDigest::from_bytes([0x55; 32]),
        language: "en-US".to_owned(),
        generated_at: UnixTimestampMilliseconds::new(1_800_000_000_000),
        speakers: vec![
            TranscriptArtifactSpeakerInput {
                speaker_id: first_speaker,
                label: "SPEAKER_00".to_owned(),
                display_name: Some("Ada".to_owned()),
            },
            TranscriptArtifactSpeakerInput {
                speaker_id: second_speaker,
                label: "SPEAKER_01".to_owned(),
                display_name: None,
            },
        ],
        segments: vec![
            TranscriptArtifactSegmentInput {
                text: "Small   habits become durable".to_owned(),
                start_milliseconds: 47_125,
                end_milliseconds: 53_000,
                speaker_id: Some(first_speaker),
                words: vec![
                    word("Small", 47_125, 47_600),
                    word("habits", 47_650, 48_100),
                ],
            },
            TranscriptArtifactSegmentInput {
                text: "when the cue is obvious.".to_owned(),
                start_milliseconds: 53_000,
                end_milliseconds: 60_000,
                speaker_id: Some(second_speaker),
                words: vec![word("when", 53_000, 53_350)],
            },
        ],
    }
}

pub(crate) fn evidence(input: &TranscriptArtifactInput) -> TranscriptEvidenceArtifact {
    build_evidence_artifact(
        &TranscriptEvidenceInput {
            episode_id: input.episode_id,
            podcast_id: input.podcast_id,
            source_revision: input.source_revision.clone(),
            source: input.source,
            provider: input.provider.clone(),
            source_payload_digest: input.source_payload_digest,
            segments: input
                .segments
                .iter()
                .map(|segment| TranscriptSegmentInput {
                    text: segment.text.clone(),
                    start_milliseconds: segment.start_milliseconds,
                    end_milliseconds: segment.end_milliseconds,
                    speaker_id: segment.speaker_id,
                })
                .collect(),
        },
        EvidenceChunkPolicy::default(),
    )
    .unwrap()
}

fn word(text: &str, start: u64, end: u64) -> TranscriptArtifactWordInput {
    TranscriptArtifactWordInput {
        text: text.to_owned(),
        start_milliseconds: start,
        end_milliseconds: end,
    }
}
