use pod0_application::{
    EvidenceChunkPolicy, TranscriptEvidenceInput, TranscriptSegmentInput, build_evidence_artifact,
};
use pod0_domain::{
    CommandId, ContentDigest, EpisodeId, PodcastId, SpeakerId, TranscriptEvidenceArtifact,
    TranscriptSource,
};

use crate::listening_import_test_support::{
    EPISODE_ID, ImportFixture, create_sqlite_source, current_metadata, episode,
};
use crate::{EvidenceStore, commit_listening_cutover};

pub(crate) struct EvidenceFixture {
    pub(crate) import: ImportFixture,
    pub(crate) store: EvidenceStore,
}

impl EvidenceFixture {
    pub(crate) fn new() -> Self {
        let import = ImportFixture::new();
        create_sqlite_source(
            &import.source,
            &current_metadata(9),
            &[episode(EPISODE_ID, "evidence-guid")],
        );
        import.stage(&import.plan()).unwrap();
        commit_listening_cutover(&import.target, 1_800_000_000_000).unwrap();
        let store = EvidenceStore::open(&import.target).unwrap();
        Self { import, store }
    }
}

pub(crate) fn artifact(revision: &str) -> TranscriptEvidenceArtifact {
    artifact_for_podcast(revision, PodcastId::from_bytes([0x11; 16]))
}

pub(crate) fn artifact_for_podcast(
    revision: &str,
    podcast_id: PodcastId,
) -> TranscriptEvidenceArtifact {
    artifact_at(revision, podcast_id, 47_125, 60_000)
}

pub(crate) fn artifact_at(
    revision: &str,
    podcast_id: PodcastId,
    start_milliseconds: u64,
    end_milliseconds: u64,
) -> TranscriptEvidenceArtifact {
    let midpoint = start_milliseconds + (end_milliseconds - start_milliseconds) / 2;
    build_evidence_artifact(
        &TranscriptEvidenceInput {
            episode_id: EpisodeId::from_bytes([0x22; 16]),
            podcast_id,
            source_revision: revision.to_owned(),
            source: TranscriptSource::Publisher,
            provider: Some("publisher-feed".to_owned()),
            source_payload_digest: ContentDigest::from_bytes([0x55; 32]),
            segments: vec![
                segment("Small habits become durable", start_milliseconds, midpoint),
                segment("when the cue is obvious.", midpoint, end_milliseconds),
            ],
        },
        EvidenceChunkPolicy::default(),
    )
    .unwrap()
}

pub(crate) fn command(value: u64) -> CommandId {
    CommandId::from_parts(0, value)
}

fn segment(text: &str, start: u64, end: u64) -> TranscriptSegmentInput {
    TranscriptSegmentInput {
        text: text.to_owned(),
        start_milliseconds: start,
        end_milliseconds: end,
        speaker_id: Some(SpeakerId::from_bytes([0x44; 16])),
    }
}
