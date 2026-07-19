use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

fn request(scope: NoteProjectionScope) -> ProjectionRequest {
    ProjectionRequest {
        scope: ProjectionScope::Notes { scope },
        offset: 0,
        max_items: 100,
    }
}

fn notes(facade: &Pod0Facade, scope: NoteProjectionScope) -> NotesProjection {
    let Projection::Notes { value } = facade.snapshot(request(scope)).projection else {
        panic!("expected notes projection");
    };
    value
}

fn envelope(id: u64, command: ApplicationCommand) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(30, id),
        cancellation_id: CancellationId::from_parts(31, id),
        expected_revision: None,
        command,
    }
}

fn operation(projection: &NotesProjection, id: u64) -> &OperationProjection {
    projection
        .operations
        .iter()
        .find(|operation| operation.command_id == CommandId::from_parts(30, id))
        .expect("note operation should be projected")
}

#[test]
fn note_commands_are_single_writer_revision_checked_and_restart_durable() {
    let fixture = PlaybackFixture::new();
    let target = Some(NoteTarget::Episode {
        episode_id: fixture.episode_id,
        position_milliseconds: 42_125,
    });
    let create = envelope(
        1,
        ApplicationCommand::CreateNote {
            text: "Remember the quiet part".to_owned(),
            kind: NoteKind::Reflection,
            author: NoteAuthor::User,
            target,
        },
    );
    fixture.facade.dispatch(create.clone());

    let created = notes(&fixture.facade, NoteProjectionScope::All);
    assert_eq!(created.notes.len(), 1);
    let note_id = NoteId::from_bytes(create.command_id.into_bytes());
    let note = &created.notes[0];
    assert_eq!(note.note_id, note_id);
    assert_eq!(note.revision, NoteRevision::INITIAL);
    assert_eq!(note.target, target);
    assert!(matches!(
        operation(&created, 1).result,
        Some(OperationResult::NoteCreated { note_id: id }) if id == note_id
    ));

    fixture.facade.dispatch(envelope(
        2,
        ApplicationCommand::UpdateNote {
            note_id,
            expected_note_revision: NoteRevision::INITIAL,
            text: "Remember the exact quiet part".to_owned(),
            kind: NoteKind::Free,
            target,
        },
    ));
    let updated = notes(
        &fixture.facade,
        NoteProjectionScope::Episode {
            episode_id: fixture.episode_id,
        },
    );
    assert_eq!(updated.notes[0].revision, NoteRevision::new(2));
    assert_eq!(updated.notes[0].text, "Remember the exact quiet part");

    fixture.facade.dispatch(envelope(
        3,
        ApplicationCommand::UpdateNote {
            note_id,
            expected_note_revision: NoteRevision::INITIAL,
            text: "A stale overwrite".to_owned(),
            kind: NoteKind::Free,
            target,
        },
    ));
    let conflicted = notes(&fixture.facade, NoteProjectionScope::All);
    assert_eq!(conflicted.notes[0].text, "Remember the exact quiet part");
    assert!(matches!(
        operation(&conflicted, 3).failure,
        Some(CoreFailure {
            code: CoreFailureCode::RevisionConflict,
            ..
        })
    ));

    fixture.facade.dispatch(envelope(
        4,
        ApplicationCommand::SetNoteDeleted {
            note_id,
            expected_note_revision: NoteRevision::new(2),
            deleted: true,
        },
    ));
    assert!(
        notes(&fixture.facade, NoteProjectionScope::Active)
            .notes
            .is_empty()
    );
    assert!(
        notes(
            &fixture.facade,
            NoteProjectionScope::Episode {
                episode_id: fixture.episode_id,
            }
        )
        .notes
        .is_empty()
    );
    assert!(notes(&fixture.facade, NoteProjectionScope::All).notes[0].deleted);

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let recovered = notes(&reopened, NoteProjectionScope::All);
    assert_eq!(recovered.notes.len(), 1);
    assert!(recovered.notes[0].deleted);
    assert_eq!(recovered.notes[0].revision, NoteRevision::new(3));

    let revision = recovered.collection_revision;
    reopened.dispatch(envelope(
        5,
        ApplicationCommand::ClearNotes {
            expected_collection_revision: revision,
        },
    ));
    let cleared = notes(&reopened, NoteProjectionScope::All);
    assert!(matches!(
        operation(&cleared, 5).result,
        Some(OperationResult::NotesCleared)
    ));
}

#[test]
fn note_validation_and_command_replay_have_typed_deterministic_outcomes() {
    let fixture = PlaybackFixture::new();
    fixture.facade.dispatch(envelope(
        10,
        ApplicationCommand::CreateNote {
            text: "   ".to_owned(),
            kind: NoteKind::Free,
            author: NoteAuthor::User,
            target: None,
        },
    ));
    let invalid = notes(&fixture.facade, NoteProjectionScope::All);
    assert!(invalid.notes.is_empty());
    assert!(matches!(
        operation(&invalid, 10).failure,
        Some(CoreFailure {
            code: CoreFailureCode::InvalidNote,
            ..
        })
    ));

    let create = envelope(
        11,
        ApplicationCommand::CreateNote {
            text: "Replay me once".to_owned(),
            kind: NoteKind::Free,
            author: NoteAuthor::Agent,
            target: None,
        },
    );
    fixture.facade.dispatch(create.clone());
    let first = notes(&fixture.facade, NoteProjectionScope::All);
    let first_revision = first.collection_revision;
    assert_eq!(first.notes.len(), 1);

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    reopened.dispatch(create);
    let replayed = notes(&reopened, NoteProjectionScope::All);
    assert_eq!(replayed.notes.len(), 1);
    assert_eq!(replayed.collection_revision, first_revision);
    assert!(matches!(
        operation(&replayed, 11).result,
        Some(OperationResult::NoteCreated { .. })
    ));
}
