use crate::runtime_playback_test_support::PlaybackFixture;
use crate::runtime_recall_test_support::{RecallFixture, evidence_input, evidence_policy};
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

#[test]
fn note_evidence_is_captured_from_the_selected_generation_and_never_retargeted() {
    let fixture = RecallFixture::new(true);
    let target = Some(NoteTarget::Episode {
        episode_id: fixture.base.episode_id,
        position_milliseconds: 15_000,
    });
    fixture.base.facade.dispatch(envelope(
        20,
        ApplicationCommand::CreateNote {
            text: "Evidence one".to_owned(),
            kind: NoteKind::Free,
            author: NoteAuthor::User,
            target,
        },
    ));
    let first = notes(&fixture.base.facade, NoteProjectionScope::All).notes[0].clone();
    let first_evidence = first.evidence.expect("selected span should be attached");
    assert_eq!(first_evidence.generation_id, fixture.artifact.generation_id);
    assert!(fixture.artifact.spans.iter().any(|span| {
        span.span_id == first_evidence.span_id
            && span.start_milliseconds <= 15_000
            && span.end_milliseconds > 15_000
    }));

    let mut next_input = evidence_input(&fixture.base);
    next_input.source_revision = "recall-fixture-v2".to_owned();
    next_input.source_payload_digest = ContentDigest::from_bytes([0x77; 32]);
    next_input.segments[0].text.push_str(" Updated.");
    let next_artifact =
        pod0_application::build_evidence_artifact(&next_input, evidence_policy()).unwrap();
    let evidence_store = pod0_storage::EvidenceStore::open(&fixture.base.target).unwrap();
    evidence_store
        .stage_artifact(
            CommandId::from_parts(70, 1),
            &next_artifact,
            1_800_000_001_000,
        )
        .unwrap();
    evidence_store
        .verify_generation(
            CommandId::from_parts(70, 2),
            next_artifact.generation_id,
            1_800_000_001_001,
        )
        .unwrap();
    evidence_store
        .select_generation(
            CommandId::from_parts(70, 3),
            fixture.base.episode_id,
            next_artifact.generation_id,
            1_800_000_001_002,
        )
        .unwrap();

    fixture.base.facade.dispatch(envelope(
        21,
        ApplicationCommand::UpdateNote {
            note_id: first.note_id,
            expected_note_revision: first.revision,
            text: "Evidence one, edited".to_owned(),
            kind: first.kind,
            target,
        },
    ));
    let updated = notes(&fixture.base.facade, NoteProjectionScope::All).notes[0].clone();
    assert_eq!(updated.evidence, Some(first_evidence));

    fixture.base.facade.dispatch(envelope(
        22,
        ApplicationCommand::CreateNote {
            text: "Evidence two".to_owned(),
            kind: NoteKind::Free,
            author: NoteAuthor::User,
            target,
        },
    ));
    let projected = notes(&fixture.base.facade, NoteProjectionScope::All);
    let second = projected
        .notes
        .iter()
        .find(|note| note.text == "Evidence two")
        .unwrap();
    assert_eq!(
        second.evidence.map(|value| value.generation_id),
        Some(next_artifact.generation_id)
    );
}
