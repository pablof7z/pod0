use pod0_application::{
    EvidenceChunkPolicy, TranscriptEvidenceInput, TranscriptSegmentInput, build_evidence_artifact,
};
use pod0_domain::StateRevision;

use crate::transcript_import_test_support::{TranscriptImportFixture, command};
use crate::transcript_store_test_support::input as replacement_input;
use crate::{EvidenceStore, TranscriptStore};

#[test]
fn evidence_remains_linked_to_imported_version_after_transcript_replacement() {
    let fixture = TranscriptImportFixture::current();
    fixture.stage(command(50)).unwrap();
    fixture.verify(command(50)).unwrap();
    fixture.commit(command(50)).unwrap();

    let transcript_store = TranscriptStore::open(&fixture.import.target).unwrap();
    let episode_id = pod0_domain::EpisodeId::from_bytes([0x22; 16]);
    let imported = transcript_store
        .selected_artifact(episode_id)
        .unwrap()
        .unwrap();
    let evidence = build_evidence_artifact(
        &TranscriptEvidenceInput {
            episode_id: imported.episode_id,
            podcast_id: imported.podcast_id,
            source_revision: imported.source_revision.clone(),
            source: imported.provenance.source,
            provider: imported.provenance.provider.clone(),
            source_payload_digest: imported.provenance.source_payload_digest,
            segments: imported
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
    .unwrap();
    assert_eq!(
        evidence.version.transcript_version_id,
        imported.transcript_version_id
    );

    let evidence_store = EvidenceStore::open(&fixture.import.target).unwrap();
    evidence_store
        .stage_artifact(command(51), &evidence, 1_800_000_000_001)
        .unwrap();
    evidence_store
        .verify_generation(command(52), evidence.generation_id, 1_800_000_000_002)
        .unwrap();
    evidence_store
        .select_generation(
            command(53),
            episode_id,
            evidence.generation_id,
            1_800_000_000_003,
        )
        .unwrap();

    let replacement = transcript_store
        .commit_and_select(
            command(54),
            StateRevision::new(1),
            replacement_input("replacement-v2"),
            1_800_000_000_004,
        )
        .unwrap();
    assert_ne!(
        replacement.transcript_version_id,
        imported.transcript_version_id
    );
    assert_eq!(
        evidence_store.selected_artifact(episode_id).unwrap(),
        Some(evidence.clone())
    );
    assert_eq!(
        evidence_store
            .selected_artifact(episode_id)
            .unwrap()
            .unwrap()
            .version
            .transcript_version_id,
        imported.transcript_version_id
    );
}
