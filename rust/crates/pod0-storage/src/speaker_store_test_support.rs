//! Fixture inputs shared by the issue #190 speaker store tests: two provider
//! runs whose speaker ids derive from the same episode and audio revision,
//! plus a disjoint-label provider for the mis-assignment scenario.

use pod0_application::transcript_speaker_id;
use pod0_domain::{
    ContentDigest, EpisodeId, PodcastId, SpeakerId, TranscriptArtifactInput,
    TranscriptArtifactSegmentInput, TranscriptArtifactSpeakerInput, TranscriptSource,
    UnixTimestampMilliseconds,
};

use crate::transcript_store_test_support::TranscriptFixture;

pub(crate) const SOURCE_REVISION: &str = "audio-revision-8f3a";
pub(crate) const BASE_MS: i64 = 1_800_000_000_900;

pub(crate) fn provider_input(run: u8) -> TranscriptArtifactInput {
    input_with_speakers(
        run,
        TranscriptSource::Scribe,
        "elevenlabs-scribe",
        ["speaker_0", "speaker_1"],
    )
}

pub(crate) fn assembly_input() -> TranscriptArtifactInput {
    input_with_speakers(3, TranscriptSource::AssemblyAi, "assembly-ai", ["A", "B"])
}

fn input_with_speakers(
    run: u8,
    source: TranscriptSource,
    provider: &str,
    labels: [&str; 2],
) -> TranscriptArtifactInput {
    let ids = labels.map(speaker_id);
    TranscriptArtifactInput {
        episode_id: episode(),
        podcast_id: PodcastId::from_bytes([0x11; 16]),
        source_revision: SOURCE_REVISION.to_owned(),
        source,
        provider: Some(provider.to_owned()),
        source_payload_digest: ContentDigest::from_bytes([0x70 + run; 32]),
        language: "en-US".to_owned(),
        generated_at: UnixTimestampMilliseconds::new(BASE_MS + i64::from(run)),
        speakers: labels
            .iter()
            .zip(ids)
            .map(|(label, speaker_id)| TranscriptArtifactSpeakerInput {
                speaker_id,
                label: (*label).to_owned(),
                display_name: None,
            })
            .collect(),
        segments: vec![
            TranscriptArtifactSegmentInput {
                text: "Small daily cues make habits durable.".to_owned(),
                start_milliseconds: 10_000,
                end_milliseconds: 20_000,
                speaker_id: Some(ids[0]),
                words: Vec::new(),
            },
            TranscriptArtifactSegmentInput {
                text: "A visible prompt reduces the effort.".to_owned(),
                start_milliseconds: 20_000,
                end_milliseconds: 31_000,
                speaker_id: Some(ids[1]),
                words: Vec::new(),
            },
        ],
    }
}

pub(crate) fn display_names(fixture: &TranscriptFixture) -> Vec<Option<String>> {
    fixture
        .store
        .selected_speakers(episode(), 0, 8)
        .unwrap()
        .items
        .into_iter()
        .map(|speaker| speaker.display_name)
        .collect()
}

pub(crate) fn speaker_id(label: &str) -> SpeakerId {
    transcript_speaker_id(episode(), SOURCE_REVISION, label).unwrap()
}

pub(crate) fn episode() -> EpisodeId {
    EpisodeId::from_bytes([0x22; 16])
}
