use pod0_domain::{CommandId, EpisodeId, NoteId, StateRevision};

use crate::{
    ActivityFact, NoteCreateActivityInput, NoteCreateMutation, RequestDisposition,
    RequestRejectionReason, plan_note_create,
};

#[test]
fn accepted_note_create_has_disposition_and_transition_facts() {
    let episode_id = EpisodeId::from_parts(3, 4);
    let plan = plan_note_create(NoteCreateActivityInput {
        command_id: CommandId::from_parts(1, 2),
        note_id: NoteId::from_parts(5, 6),
        episode_id: Some(episode_id),
        current_revision: StateRevision::new(7),
        committed_revision: StateRevision::new(11),
        disposition: RequestDisposition::Accepted,
    })
    .unwrap();

    assert_eq!(plan.disposition(), RequestDisposition::Accepted);
    let (_, expected, mutation, facts, effects, commands, _) = plan.into_parts();
    assert_eq!(expected, StateRevision::new(7));
    assert_eq!(mutation, NoteCreateMutation::Apply);
    assert_eq!(facts.len(), 2);
    assert_eq!(facts.get(0).unwrap().episode_id, Some(episode_id));
    assert!(matches!(
        facts.get(1).unwrap().fact,
        ActivityFact::DomainTransition { .. }
    ));
    assert!(effects.is_empty());
    assert!(commands.is_empty());
}

#[test]
fn rejected_and_duplicate_note_create_have_no_state_mutation() {
    for disposition in [
        RequestDisposition::Rejected {
            reason: RequestRejectionReason::Invalid,
        },
        RequestDisposition::Duplicate,
    ] {
        let plan = plan_note_create(NoteCreateActivityInput {
            command_id: CommandId::from_parts(1, 2),
            note_id: NoteId::from_parts(5, 6),
            episode_id: None,
            current_revision: StateRevision::new(7),
            committed_revision: StateRevision::new(7),
            disposition,
        })
        .unwrap();
        assert_eq!(plan.disposition(), disposition);
        let (_, _, mutation, facts, _, _, _) = plan.into_parts();
        assert_eq!(mutation, NoteCreateMutation::None);
        assert_eq!(facts.len(), 1);
    }
}
