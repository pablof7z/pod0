//! Issue #190: durable speaker identity.
//!
//! The decided model: a mutable, artifact-external speaker entity
//! (`pod0_speakers`) plus a revisable link (`pod0_speaker_assignments`)
//! keyed on `(artifact_id, speaker_id)` with completion-time carry-forward.
//! A carried assignment is written with `origin = inferred` so it stays
//! visibly revisable, while a user-set assignment stays authoritative.
//! Neither table couples to artifact lifecycle via foreign keys, matching
//! the `pod0_category_members` prior art.
//!
//! The schema test below is the red test for the migration that follows
//! 0035 (reserved by the in-flight feed-fetch branch). The identity tests
//! pin the derivation invariants the carry-forward mechanism relies on:
//! a same-provider re-transcription mints identical speaker ids inside a
//! new artifact (so carrying assignments forward is well defined), and a
//! different provider's labels mint disjoint ids (so its speakers must
//! come back unassigned rather than aliased onto the wrong entity).

use pod0_application::transcript_speaker_id;
use pod0_domain::{
    ContentDigest, EpisodeId, PodcastId, SpeakerId, StateRevision, TranscriptArtifactInput,
    TranscriptArtifactSegmentInput, TranscriptArtifactSpeakerInput, TranscriptSource,
    UnixTimestampMilliseconds,
};
use rusqlite::Connection;

use crate::transcript_store_test_support::{TranscriptFixture, command};

const SOURCE_REVISION: &str = "audio-revision-8f3a";

/// RED: the speaker entity and assignment tables do not exist yet.
///
/// The `artifact_id` column in the assignment table is load-bearing: it
/// encodes the decided key `(artifact_id, speaker_id)`. Keying on
/// `speaker_id` alone would silently carry a user-authority name onto a
/// possibly different person when diarization indices permute across runs.
#[test]
fn speaker_entity_and_assignment_tables_exist_with_artifact_scoped_key() {
    let fixture = TranscriptFixture::new();
    let connection = Connection::open(&fixture.import.target).unwrap();
    for table in ["pod0_speakers", "pod0_speaker_assignments"] {
        assert_eq!(
            table_count(&connection, table),
            1,
            "missing table {table}: the speaker identity migration for issue #190 \
             has not been applied"
        );
    }
    for column in ["speaker_entity_id", "display_name"] {
        assert_eq!(
            column_count(&connection, "pod0_speakers", column),
            1,
            "pod0_speakers must carry column {column}"
        );
    }
    for column in [
        "artifact_id",
        "speaker_id",
        "speaker_entity_id",
        "origin_code",
        "decided_at_ms",
    ] {
        assert_eq!(
            column_count(&connection, "pod0_speaker_assignments", column),
            1,
            "pod0_speaker_assignments must carry column {column}; assignments are \
             keyed (artifact_id, speaker_id) with origin_code 1|2|3 for \
             user|inferred|feed_metadata"
        );
    }
}

/// PIN (passes today): a same-provider re-transcription of the same audio
/// revision mints the *same* speaker ids inside a *new* artifact.
///
/// This is the precondition that makes completion-time carry-forward well
/// defined for scenario "a name given once survives re-transcription": the
/// new artifact's `(artifact_id, speaker_id)` rows can be seeded from the
/// superseded artifact's assignments by matching `speaker_id`, written with
/// `origin = inferred`.
#[test]
fn same_provider_retranscription_mints_identical_speaker_ids_in_a_new_artifact() {
    let fixture = TranscriptFixture::new();
    let first = fixture
        .store
        .commit_and_select(
            command(910),
            StateRevision::INITIAL,
            scribe_input(1),
            1_800_000_000_910,
        )
        .unwrap();
    let first_ids = selected_speaker_ids(&fixture);
    let second = fixture
        .store
        .commit_and_select(
            command(911),
            StateRevision::new(1),
            scribe_input(2),
            1_800_000_000_911,
        )
        .unwrap();
    assert_ne!(
        second.artifact_id, first.artifact_id,
        "a re-transcription must seal a distinct artifact"
    );
    assert_eq!(
        selected_speaker_ids(&fixture),
        first_ids,
        "same provider, same audio revision, same labels must mint identical \
         speaker ids so assignments can carry forward across artifacts"
    );
}

/// PIN (passes today): a different provider's labels mint disjoint speaker
/// ids for the same episode and audio revision.
///
/// This is the precondition for scenario "a new provider does not silently
/// mis-assign": no `(artifact_id, speaker_id)` row from the old provider's
/// artifact can match the new artifact's speakers, so they surface
/// unassigned while the named entity survives untouched.
#[test]
fn different_provider_labels_mint_disjoint_speaker_ids() {
    let scribe = [speaker_id("speaker_0"), speaker_id("speaker_1")];
    let assembly = [speaker_id("A"), speaker_id("B")];
    for id in scribe {
        assert!(
            !assembly.contains(&id),
            "cross-provider labels must never alias onto the same speaker id"
        );
    }
}

fn scribe_input(run: u8) -> TranscriptArtifactInput {
    let first = speaker_id("speaker_0");
    let second = speaker_id("speaker_1");
    TranscriptArtifactInput {
        episode_id: episode(),
        podcast_id: PodcastId::from_bytes([0x11; 16]),
        source_revision: SOURCE_REVISION.to_owned(),
        source: TranscriptSource::Scribe,
        provider: Some("elevenlabs-scribe".to_owned()),
        source_payload_digest: ContentDigest::from_bytes([0x60 + run; 32]),
        language: "en-US".to_owned(),
        generated_at: UnixTimestampMilliseconds::new(1_800_000_000_900 + i64::from(run)),
        speakers: vec![
            TranscriptArtifactSpeakerInput {
                speaker_id: first,
                label: "speaker_0".to_owned(),
                display_name: None,
            },
            TranscriptArtifactSpeakerInput {
                speaker_id: second,
                label: "speaker_1".to_owned(),
                display_name: None,
            },
        ],
        segments: vec![
            TranscriptArtifactSegmentInput {
                text: "Small daily cues make habits durable.".to_owned(),
                start_milliseconds: 10_000,
                end_milliseconds: 20_000,
                speaker_id: Some(first),
                words: Vec::new(),
            },
            TranscriptArtifactSegmentInput {
                text: "A visible prompt reduces the effort.".to_owned(),
                start_milliseconds: 20_000,
                end_milliseconds: 31_000,
                speaker_id: Some(second),
                words: Vec::new(),
            },
        ],
    }
}

fn selected_speaker_ids(fixture: &TranscriptFixture) -> Vec<SpeakerId> {
    fixture
        .store
        .selected_speakers(episode(), 0, 8)
        .unwrap()
        .items
        .into_iter()
        .map(|speaker| speaker.speaker_id)
        .collect()
}

fn speaker_id(label: &str) -> SpeakerId {
    transcript_speaker_id(episode(), SOURCE_REVISION, label).unwrap()
}

fn episode() -> EpisodeId {
    EpisodeId::from_bytes([0x22; 16])
}

fn table_count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |row| row.get(0),
        )
        .unwrap()
}

fn column_count(connection: &Connection, table: &str, column: &str) -> i64 {
    connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name=?2",
            [table, column],
            |row| row.get(0),
        )
        .unwrap()
}
