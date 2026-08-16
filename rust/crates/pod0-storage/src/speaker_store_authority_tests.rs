use super::*;

#[test]
fn user_assignments_outrank_inferred_writes_and_bad_input_fails_closed() {
    let fixture = TranscriptFixture::new();
    let receipt = fixture
        .store
        .commit_and_select(command(925), StateRevision::INITIAL, provider_input(1), BASE_MS)
        .unwrap();
    let named_by_user = SpeakerEntityId::from_parts(9, 4);
    let guessed = SpeakerEntityId::from_parts(9, 5);
    for (index, (entity, name)) in [(named_by_user, "Ada Lovelace"), (guessed, "Grace Hopper")]
        .into_iter()
        .enumerate()
    {
        fixture
            .store
            .create_speaker_entity(
                command(1_030 + index as u64),
                &fp(if index == 0 { '1' } else { '2' }),
                entity,
                name,
                BASE_MS,
            )
            .unwrap();
    }
    let speaker = speaker_id("speaker_0");
    fixture
        .store
        .assign_speaker(
            command(1_032),
            &fp('3'),
            receipt.artifact_id,
            speaker,
            named_by_user,
            SpeakerAssignmentOrigin::User,
            None,
            BASE_MS,
        )
        .unwrap();
    assert_eq!(
        fixture.store.assign_speaker(
            command(1_033),
            &fp('4'),
            receipt.artifact_id,
            speaker,
            guessed,
            SpeakerAssignmentOrigin::Inferred,
            Some(0.9),
            BASE_MS + 1,
        ),
        Err(StorageError::InvalidSpeakerEntity),
        "automatic inference cannot override a user-authoritative assignment"
    );
    let assignments = fixture.store.speaker_assignments(receipt.artifact_id).unwrap();
    assert_eq!(assignments[0].speaker_entity_id, named_by_user);
    assert_eq!(assignments[0].origin, SpeakerAssignmentOrigin::User);

    assert_eq!(
        fixture.store.create_speaker_entity(
            command(1_034),
            &fp('5'),
            SpeakerEntityId::from_parts(9, 6),
            "   ",
            BASE_MS,
        ),
        Err(StorageError::InvalidSpeakerEntity)
    );
    assert_eq!(
        fixture.store.rename_speaker_entity(
            command(1_035),
            &fp('6'),
            SpeakerEntityId::from_parts(9, 7),
            1,
            "Nobody",
            BASE_MS,
        ),
        Err(StorageError::EntityNotFound)
    );
    assert_eq!(
        fixture.store.assign_speaker(
            command(1_036),
            &fp('7'),
            receipt.artifact_id,
            speaker_id("missing"),
            named_by_user,
            SpeakerAssignmentOrigin::User,
            None,
            BASE_MS,
        ),
        Err(StorageError::EntityNotFound)
    );
    assert_eq!(
        fixture.store.assign_speaker(
            command(1_037),
            &fp('8'),
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

#[test]
fn replay_and_stale_speaker_edits_are_deterministic_and_durable() {
    let fixture = TranscriptFixture::new();
    let entity = SpeakerEntityId::from_parts(9, 50);
    let create = command(1_050);
    let created = fixture
        .store
        .create_speaker_entity(create, &fp('a'), entity, "Ada", BASE_MS)
        .unwrap();
    assert_eq!(
        fixture
            .store
            .create_speaker_entity(create, &fp('a'), entity, "Ada", BASE_MS + 1)
            .unwrap(),
        created,
        "same command and fingerprint must replay its transition receipt"
    );
    fixture
        .store
        .rename_speaker_entity(
            command(1_051),
            &fp('b'),
            entity,
            1,
            "Ada Lovelace",
            BASE_MS + 2,
        )
        .unwrap();
    let stale = command(1_052);
    assert_eq!(
        fixture.store.rename_speaker_entity(
            stale,
            &fp('c'),
            entity,
            1,
            "Stale Name",
            BASE_MS + 3,
        ),
        Err(StorageError::RevisionConflict)
    );
    assert_eq!(
        fixture
            .store
            .speaker_entity(entity)
            .unwrap()
            .unwrap()
            .display_name,
        "Ada Lovelace"
    );
    let facts = ActivityStore::open(&fixture.import.target)
        .unwrap()
        .page_for_correlation(
            CommandActivityIdentity::new(stale).correlation_id(),
            None,
            10,
        )
        .unwrap();
    assert_eq!(facts.items.len(), 1);
    assert!(matches!(
        facts.items[0].draft.fact,
        ActivityFact::RequestDisposition {
            disposition: RequestDisposition::Rejected {
                reason: RequestRejectionReason::RevisionConflict
            }
        }
    ));
}
