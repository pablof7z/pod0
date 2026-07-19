use rusqlite::Connection;
use serde_json::Value;

use crate::legacy_transcript_source::{
    inspect_transcript_source, load_inspected_transcript_artifact,
};
use crate::transcript_import_test_support::*;
use crate::transcript_legacy_backup::artifact_backup_path;
use crate::{
    LegacyTranscriptSourceKind, TranscriptImportState, TranscriptStore, read_transcript_import,
    read_transcript_import_entries,
};

#[test]
fn selected_transcript_is_backed_up_staged_verified_committed_and_reopened_losslessly() {
    let fixture = TranscriptImportFixture::current();
    let plan = fixture.plan();
    assert_eq!(
        plan.source_kind,
        LegacyTranscriptSourceKind::ArtifactSqliteV1
    );
    assert_eq!(plan.source_generation, 12);
    assert_eq!(plan.selected_count, 1);

    let staged = fixture.stage(command(10)).unwrap();
    assert_eq!(staged.state, TranscriptImportState::Staged);
    assert!(!staged.backup.reused_database);
    assert_eq!(staged.backup.artifact_count, 1);
    let episode_id = pod0_domain::EpisodeId::from_bytes([0x22; 16]);
    let original_bytes = std::fs::read(&fixture.selected_path).unwrap();
    let original_digest = crate::transcript_import_digest::digest_bytes(&original_bytes);
    assert_eq!(
        std::fs::read(artifact_backup_path(
            &fixture.backup_root,
            episode_id,
            original_digest,
        ))
        .unwrap(),
        original_bytes
    );
    let store = TranscriptStore::open(&fixture.import.target).unwrap();
    assert_eq!(store.selected_artifact(episode_id).unwrap(), None);
    assert_eq!(cutover_state(&fixture), Some("staged".to_owned()));

    let replayed_stage = fixture.stage(command(10)).unwrap();
    assert_eq!(replayed_stage.plan, staged.plan);
    assert!(replayed_stage.reused_existing);
    assert!(replayed_stage.backup.reused_database);
    let verified = fixture.verify(command(10)).unwrap();
    assert_eq!(verified.report.state, TranscriptImportState::Verified);
    assert_eq!(verified.verified_artifact_count, 1);
    assert_eq!(verified.verified_segment_count, 2);
    assert_eq!(verified.verified_word_count, 2);
    let entries =
        read_transcript_import_entries(&fixture.import.target, command(10), 0, 1).unwrap();
    assert_eq!(entries.items.len(), 1);
    assert!(!entries.has_more);
    let imported_entry = entries.items[0];
    assert_eq!(imported_entry.episode_id, episode_id);
    assert_eq!(
        imported_entry.selected_file_digest,
        crate::transcript_import_digest::digest_bytes(
            &std::fs::read(&fixture.selected_path).unwrap()
        )
    );
    assert_eq!(store.selected_artifact(episode_id).unwrap(), None);

    let committed = fixture.commit(command(10)).unwrap();
    assert_eq!(committed.state, TranscriptImportState::Committed);
    assert_eq!(committed.target_revision.value, 1);
    let artifact = store.selected_artifact(episode_id).unwrap().unwrap();
    assert_eq!(artifact.artifact_id, imported_entry.artifact_id);
    assert_eq!(
        artifact.transcript_version_id,
        imported_entry.transcript_version_id
    );
    assert_eq!(
        artifact.content_digest,
        imported_entry.transcript_content_digest
    );
    assert_eq!(artifact.language, "en-US");
    assert_eq!(artifact.generated_at.value, 1_800_000_000_000);
    assert_eq!(
        artifact.provenance.source,
        pod0_domain::TranscriptSource::Publisher
    );
    assert_eq!(artifact.speakers[0].label, "SPEAKER_00");
    assert_eq!(artifact.speakers[0].display_name.as_deref(), Some("Ada"));
    assert_eq!(artifact.segments[0].start_milliseconds, 47_125);
    assert_eq!(artifact.segments[0].end_milliseconds, 53_000);
    assert_eq!(artifact.segments[0].words.len(), 2);
    assert_eq!(artifact.segments[0].words[0].start_milliseconds, 47_125);
    assert_eq!(artifact.segments[0].words[0].end_milliseconds, 47_600);
    assert_eq!(artifact.segments[0].words[1].start_milliseconds, 47_650);
    assert_eq!(artifact.segments[0].words[1].end_milliseconds, 48_100);
    assert!(artifact.segments[1].words.is_empty());
    assert_eq!(artifact.provenance.provider, None);
    assert_eq!(
        artifact.source_revision,
        format!(
            "selected-json-sha256:{}",
            hex(artifact.provenance.source_payload_digest)
        )
    );
    artifact.verify_integrity().unwrap();
    assert_eq!(cutover_state(&fixture), Some("staged".to_owned()));
    let replayed_commit = fixture.commit(command(10)).unwrap();
    assert_eq!(replayed_commit.plan, committed.plan);
    assert_eq!(replayed_commit.target_revision, committed.target_revision);
    assert_eq!(replayed_commit.state, committed.state);
    assert!(replayed_commit.reused_existing);
    assert!(replayed_commit.backup.reused_database);
    assert_eq!(replayed_commit.backup.reused_artifacts, 1);
    assert_eq!(
        read_transcript_import(&fixture.import.target, command(10))
            .unwrap()
            .state,
        TranscriptImportState::Committed
    );

    let reopened = TranscriptStore::open(&fixture.import.target).unwrap();
    assert_eq!(
        reopened.selected_artifact(episode_id).unwrap(),
        Some(artifact)
    );
}

#[test]
fn supported_optional_swift_fields_import_when_absent() {
    let fixture = TranscriptImportFixture::current();
    let mut raw: Value =
        serde_json::from_slice(&std::fs::read(&fixture.selected_path).unwrap()).unwrap();
    raw["segments"][0].as_object_mut().unwrap().remove("words");
    raw["segments"][0]
        .as_object_mut()
        .unwrap()
        .remove("speakerID");
    raw["speakers"][0]
        .as_object_mut()
        .unwrap()
        .remove("displayName");
    fixture.replace_selected_bytes(&serde_json::to_vec(&raw).unwrap());

    fixture.stage(command(11)).unwrap();
    fixture.verify(command(11)).unwrap();
    fixture.commit(command(11)).unwrap();
    let artifact = TranscriptStore::open(&fixture.import.target)
        .unwrap()
        .selected_artifact(pod0_domain::EpisodeId::from_bytes([0x22; 16]))
        .unwrap()
        .unwrap();
    assert!(artifact.segments[0].words.is_empty());
    assert_eq!(artifact.segments[0].speaker_id, None);
    assert_eq!(artifact.speakers[0].display_name, None);
}

#[test]
fn already_current_shared_artifact_is_adopted_without_duplicate_selection() {
    let fixture = TranscriptImportFixture::current();
    let inspected =
        inspect_transcript_source(&fixture.import.source, &fixture.transcript_root).unwrap();
    let legacy_artifact = load_inspected_transcript_artifact(&inspected.entries[0], 0).unwrap();
    let store = TranscriptStore::open(&fixture.import.target).unwrap();
    store
        .commit_and_select(
            command(12),
            pod0_domain::StateRevision::INITIAL,
            artifact_input(&legacy_artifact),
            1_800_000_000_000,
        )
        .unwrap();

    let staged = fixture.stage(command(13)).unwrap();
    assert_eq!(staged.target_revision.value, 2);
    fixture.verify(command(13)).unwrap();
    fixture.commit(command(13)).unwrap();
    assert_eq!(
        store
            .selected_summary(legacy_artifact.episode_id)
            .unwrap()
            .unwrap()
            .selection_revision
            .value,
        1
    );
    assert_eq!(
        store.selected_artifact(legacy_artifact.episode_id).unwrap(),
        Some(legacy_artifact)
    );
}

#[test]
fn legacy_v0_selection_matches_swift_winner_policy_and_empty_source_is_valid() {
    let fixture = TranscriptImportFixture::legacy_v0();
    let plan = fixture.plan();
    assert_eq!(
        plan.source_kind,
        LegacyTranscriptSourceKind::ArtifactSqliteV0
    );
    assert_eq!(plan.selected_count, 1);
    fixture.stage(command(20)).unwrap();
    fixture.verify(command(20)).unwrap();
    fixture.commit(command(20)).unwrap();
    assert!(
        TranscriptStore::open(&fixture.import.target)
            .unwrap()
            .selected_artifact(pod0_domain::EpisodeId::from_bytes([0x22; 16]))
            .unwrap()
            .is_some()
    );

    let empty = TranscriptImportFixture::current();
    Connection::open(&empty.import.source)
        .unwrap()
        .execute("DELETE FROM artifacts", [])
        .unwrap();
    assert_eq!(empty.plan().selected_count, 0);
    empty.stage(command(21)).unwrap();
    let verification = empty.verify(command(21)).unwrap();
    assert_eq!(verification.verified_artifact_count, 0);
    assert_eq!(
        empty.commit(command(21)).unwrap().state,
        TranscriptImportState::Committed
    );
}

fn cutover_state(fixture: &TranscriptImportFixture) -> Option<String> {
    Connection::open(&fixture.import.target)
        .unwrap()
        .query_row(
            "SELECT state FROM pod0_domain_cutovers WHERE domain='transcripts'",
            [],
            |row| row.get(0),
        )
        .ok()
}

fn hex(value: pod0_domain::ContentDigest) -> String {
    value
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn artifact_input(
    artifact: &pod0_domain::TranscriptArtifact,
) -> pod0_domain::TranscriptArtifactInput {
    pod0_domain::TranscriptArtifactInput {
        episode_id: artifact.episode_id,
        podcast_id: artifact.podcast_id,
        source_revision: artifact.source_revision.clone(),
        source: artifact.provenance.source,
        provider: artifact.provenance.provider.clone(),
        source_payload_digest: artifact.provenance.source_payload_digest,
        language: artifact.language.clone(),
        generated_at: artifact.generated_at,
        speakers: artifact.speakers.clone(),
        segments: artifact
            .segments
            .iter()
            .map(|segment| pod0_domain::TranscriptArtifactSegmentInput {
                text: segment.text.clone(),
                start_milliseconds: segment.start_milliseconds,
                end_milliseconds: segment.end_milliseconds,
                speaker_id: segment.speaker_id,
                words: segment.words.clone(),
            })
            .collect(),
    }
}
