//! Issue #190: the speaker entity/assignment store and its completion-time
//! carry-forward, exercised through the same `selected_speakers` read the
//! transcript projection and agent recall share.

use pod0_application::{
    ActivityFact, CommandActivityIdentity, DomainTransitionKind, RequestDisposition,
    RequestRejectionReason, UserArtifactTransition,
};
use pod0_domain::{SpeakerEntityId, StateRevision};

use crate::{ActivityStore, StorageError};
use crate::speaker_store_model::SpeakerAssignmentOrigin;
use crate::speaker_store_test_support::{
    BASE_MS, assembly_input, display_names, provider_input, speaker_id,
};
use crate::transcript_store_test_support::{TranscriptFixture, command};

#[path = "speaker_store_authority_tests.rs"]
mod authority;

fn fp(digit: char) -> String {
    digit.to_string().repeat(64)
}

#[test]
fn rename_reaches_the_shared_selected_speakers_read_and_keeps_unassigned_names() {
    let fixture = TranscriptFixture::new();
    let receipt = fixture
        .store
        .commit_and_select(command(920), StateRevision::INITIAL, provider_input(1), BASE_MS)
        .unwrap();
    let entity = SpeakerEntityId::from_parts(9, 1);
    fixture
        .store
        .create_speaker_entity(command(1_001), &fp('1'), entity, "Ada Lovelace", BASE_MS)
        .unwrap();
    fixture
        .store
        .assign_speaker(
            command(1_002),
            &fp('2'),
            receipt.artifact_id,
            speaker_id("speaker_0"),
            entity,
            SpeakerAssignmentOrigin::User,
            None,
            BASE_MS,
        )
        .unwrap();
    let activity = ActivityStore::open(&fixture.import.target)
        .unwrap()
        .page_for_episode(crate::speaker_store_test_support::episode(), None, 200)
        .unwrap();
    assert!(activity.items.iter().any(|item| {
        matches!(
            item.draft.fact,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::UserArtifact(
                    UserArtifactTransition::SpeakerAssignmentChanged
                ),
                ..
            }
        )
    }));
    let journal_payload: String = crate::migration_db::open_connection(&fixture.import.target, true)
        .unwrap()
        .query_row(
            "SELECT GROUP_CONCAT(payload_json,'') FROM pod0_activity_facts",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        !journal_payload.contains("Ada Lovelace"),
        "activity facts must retain stable identities and bounded action codes, not private names"
    );
    assert_eq!(
        display_names(&fixture),
        [Some("Ada Lovelace".to_owned()), None],
        "an assigned entity name must override the artifact-sealed one while \
         an unassigned speaker keeps its artifact value"
    );
    fixture
        .store
        .rename_speaker_entity(
            command(1_003),
            &fp('3'),
            entity,
            1,
            "Countess Lovelace",
            BASE_MS + 5,
        )
        .unwrap();
    assert_eq!(
        display_names(&fixture),
        [Some("Countess Lovelace".to_owned()), None],
        "a rename must reach every reader through the shared join"
    );
    assert_eq!(
        fixture.store.speaker_entity(entity).unwrap().unwrap().revision,
        2
    );
}

#[test]
fn same_provider_recommit_carries_assignments_forward_as_inferred() {
    let fixture = TranscriptFixture::new();
    let first = fixture
        .store
        .commit_and_select(command(921), StateRevision::INITIAL, provider_input(1), BASE_MS)
        .unwrap();
    let entity = SpeakerEntityId::from_parts(9, 2);
    fixture
        .store
        .create_speaker_entity(command(1_010), &fp('1'), entity, "Grace Hopper", BASE_MS)
        .unwrap();
    fixture
        .store
        .assign_speaker(
            command(1_011),
            &fp('2'),
            first.artifact_id,
            speaker_id("speaker_1"),
            entity,
            SpeakerAssignmentOrigin::User,
            Some(1.0),
            BASE_MS,
        )
        .unwrap();
    let second = fixture
        .store
        .commit_and_select(
            command(922),
            StateRevision::new(1),
            provider_input(2),
            BASE_MS + 10,
        )
        .unwrap();
    assert_ne!(second.artifact_id, first.artifact_id);
    let carried = fixture.store.speaker_assignments(second.artifact_id).unwrap();
    assert_eq!(carried.len(), 1);
    assert_eq!(carried[0].speaker_entity_id, entity);
    assert_eq!(
        carried[0].origin,
        SpeakerAssignmentOrigin::Inferred,
        "a carried assignment must never claim user authority: diarization \
         indices can permute within a provider across runs"
    );
    assert_eq!(
        display_names(&fixture),
        [None, Some("Grace Hopper".to_owned())],
        "the name given once must survive re-transcription without re-naming"
    );
}

#[test]
fn different_provider_labels_surface_unassigned_while_the_entity_survives() {
    let fixture = TranscriptFixture::new();
    let first = fixture
        .store
        .commit_and_select(command(923), StateRevision::INITIAL, provider_input(1), BASE_MS)
        .unwrap();
    let entity = SpeakerEntityId::from_parts(9, 3);
    fixture
        .store
        .create_speaker_entity(command(1_020), &fp('1'), entity, "Ada Lovelace", BASE_MS)
        .unwrap();
    fixture
        .store
        .assign_speaker(
            command(1_021),
            &fp('2'),
            first.artifact_id,
            speaker_id("speaker_0"),
            entity,
            SpeakerAssignmentOrigin::User,
            None,
            BASE_MS,
        )
        .unwrap();
    let second = fixture
        .store
        .commit_and_select(
            command(924),
            StateRevision::new(1),
            assembly_input(),
            BASE_MS + 10,
        )
        .unwrap();
    assert!(
        fixture
            .store
            .speaker_assignments(second.artifact_id)
            .unwrap()
            .is_empty(),
        "disjoint provider labels must come back unassigned, never aliased"
    );
    assert_eq!(display_names(&fixture), [None, None]);
    let survivor = fixture.store.speaker_entity(entity).unwrap().unwrap();
    assert_eq!(survivor.display_name, "Ada Lovelace");
    assert!(!survivor.deleted);
}
