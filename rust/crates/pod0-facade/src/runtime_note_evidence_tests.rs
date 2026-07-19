use crate::runtime_recall_test_support::{RecallFixture, evidence_input, evidence_policy};
use crate::*;

fn notes(facade: &Pod0Facade) -> NotesProjection {
    let Projection::Notes { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Notes {
                scope: NoteProjectionScope::All,
            },
            offset: 0,
            max_items: 100,
        })
        .projection
    else {
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

#[test]
fn note_evidence_survives_replacement_restart_and_unsubscribe_without_retargeting() {
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
    let first = notes(&fixture.base.facade).notes[0].clone();
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
    let next = pod0_application::build_evidence_artifact(&next_input, evidence_policy()).unwrap();
    let store = pod0_storage::EvidenceStore::open(&fixture.base.target).unwrap();
    store
        .stage_artifact(CommandId::from_parts(70, 1), &next, 1_800_000_001_000)
        .unwrap();
    store
        .verify_generation(
            CommandId::from_parts(70, 2),
            next.generation_id,
            1_800_000_001_001,
        )
        .unwrap();
    store
        .select_generation(
            CommandId::from_parts(70, 3),
            fixture.base.episode_id,
            next.generation_id,
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
    let updated = notes(&fixture.base.facade).notes[0].clone();
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
    assert_eq!(
        notes(&fixture.base.facade)
            .notes
            .iter()
            .find(|note| note.text == "Evidence two")
            .and_then(|note| note.evidence)
            .map(|value| value.generation_id),
        Some(next.generation_id)
    );

    fixture.base.facade.dispatch(envelope(
        23,
        ApplicationCommand::Unsubscribe {
            podcast_id: fixture.base.podcast_id,
        },
    ));
    let reopened = Pod0Facade::open(fixture.base.target.to_string_lossy().into_owned()).unwrap();
    let recovered = notes(&reopened);
    let recovered_first = recovered
        .notes
        .iter()
        .find(|note| note.note_id == first.note_id)
        .unwrap();
    assert_eq!(recovered_first.text, "Evidence one, edited");
    assert_eq!(recovered_first.evidence, Some(first_evidence));
    assert_eq!(
        recovered
            .notes
            .iter()
            .find(|note| note.text == "Evidence two")
            .and_then(|note| note.evidence)
            .map(|value| value.generation_id),
        Some(next.generation_id)
    );
    let reopened_store = pod0_storage::EvidenceStore::open(&fixture.base.target).unwrap();
    assert!(
        reopened_store
            .generation(fixture.artifact.generation_id)
            .unwrap()
            .is_some()
    );
    assert!(
        reopened_store
            .generation(next.generation_id)
            .unwrap()
            .is_some()
    );
}
