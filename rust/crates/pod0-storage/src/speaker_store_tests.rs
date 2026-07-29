//! Issue #190: the speaker entity/assignment store and its completion-time
//! carry-forward, exercised through the same `selected_speakers` read the
//! transcript projection and agent recall share.

use pod0_domain::{SpeakerEntityId, StateRevision};

use crate::StorageError;
use crate::speaker_store_model::SpeakerAssignmentOrigin;
use crate::speaker_store_test_support::{
    BASE_MS, assembly_input, display_names, provider_input, speaker_id,
};
use crate::transcript_store_test_support::{TranscriptFixture, command};

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
        .create_speaker_entity(entity, "Ada Lovelace", None, BASE_MS)
        .unwrap();
    fixture
        .store
        .assign_speaker(
            receipt.artifact_id,
            speaker_id("speaker_0"),
            entity,
            SpeakerAssignmentOrigin::User,
            None,
            BASE_MS,
        )
        .unwrap();
    assert_eq!(
        display_names(&fixture),
        [Some("Ada Lovelace".to_owned()), None],
        "an assigned entity name must override the artifact-sealed one while \
         an unassigned speaker keeps its artifact value"
    );
    fixture
        .store
        .rename_speaker_entity(entity, "Countess Lovelace", BASE_MS + 5)
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
        .create_speaker_entity(entity, "Grace Hopper", None, BASE_MS)
        .unwrap();
    fixture
        .store
        .assign_speaker(
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
        .create_speaker_entity(entity, "Ada Lovelace", None, BASE_MS)
        .unwrap();
    fixture
        .store
        .assign_speaker(
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

#[test]
fn user_assignments_outrank_inferred_writes_and_bad_input_fails_closed() {
    let fixture = TranscriptFixture::new();
    let receipt = fixture
        .store
        .commit_and_select(command(925), StateRevision::INITIAL, provider_input(1), BASE_MS)
        .unwrap();
    let named_by_user = SpeakerEntityId::from_parts(9, 4);
    let guessed = SpeakerEntityId::from_parts(9, 5);
    for (entity, name) in [(named_by_user, "Ada Lovelace"), (guessed, "Grace Hopper")] {
        fixture
            .store
            .create_speaker_entity(entity, name, None, BASE_MS)
            .unwrap();
    }
    let speaker = speaker_id("speaker_0");
    fixture
        .store
        .assign_speaker(
            receipt.artifact_id,
            speaker,
            named_by_user,
            SpeakerAssignmentOrigin::User,
            None,
            BASE_MS,
        )
        .unwrap();
    fixture
        .store
        .assign_speaker(
            receipt.artifact_id,
            speaker,
            guessed,
            SpeakerAssignmentOrigin::Inferred,
            Some(0.9),
            BASE_MS + 1,
        )
        .unwrap();
    let assignments = fixture.store.speaker_assignments(receipt.artifact_id).unwrap();
    assert_eq!(assignments[0].speaker_entity_id, named_by_user);
    assert_eq!(assignments[0].origin, SpeakerAssignmentOrigin::User);

    assert_eq!(
        fixture.store.create_speaker_entity(
            SpeakerEntityId::from_parts(9, 6),
            "   ",
            None,
            BASE_MS,
        ),
        Err(StorageError::InvalidSpeakerEntity)
    );
    assert_eq!(
        fixture
            .store
            .rename_speaker_entity(SpeakerEntityId::from_parts(9, 7), "Nobody", BASE_MS),
        Err(StorageError::EntityNotFound)
    );
    assert_eq!(
        fixture.store.assign_speaker(
            receipt.artifact_id,
            speaker_id("missing"),
            named_by_user,
            SpeakerAssignmentOrigin::User,
            None,
            BASE_MS,
        ),
        Err(StorageError::TranscriptNotFound)
    );
    assert_eq!(
        fixture.store.assign_speaker(
            receipt.artifact_id,
            speaker,
            named_by_user,
            SpeakerAssignmentOrigin::User,
            Some(1.5),
            BASE_MS,
        ),
        Err(StorageError::InvalidSpeakerEntity)
    );
}
