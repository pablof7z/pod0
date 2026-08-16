use pod0_domain::{CommandId, StateRevision};

use crate::{
    ActivityFact, DomainTransitionKind, LifecycleTransition, ListeningResetActivityInput,
    ListeningResetMutation, RequestDisposition, plan_listening_reset,
};

#[test]
fn reset_is_a_lifecycle_erasure_not_a_playback_transition() {
    let plan = plan_listening_reset(ListeningResetActivityInput {
        command_id: CommandId::from_parts(0, 1),
        current_revision: StateRevision::new(7),
        legacy_command_revision: None,
        effects: Vec::new(),
        superseded_effects: Vec::new(),
    })
    .expect("plan reset");
    let (_, _, mutation, facts, _, _, disposition) = plan.into_parts();
    assert_eq!(mutation, ListeningResetMutation::Reset);
    assert_eq!(disposition, RequestDisposition::Accepted);
    assert!(facts.iter().any(|draft| matches!(
        draft.fact,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::Lifecycle(LifecycleTransition::UserDataErasureChanged),
            ..
        }
    )));
    assert!(facts.iter().all(|draft| !matches!(
        draft.fact,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::Playback(_),
            ..
        }
    )));
}

#[test]
fn duplicate_reset_has_no_transition_or_effect() {
    let plan = plan_listening_reset(ListeningResetActivityInput {
        command_id: CommandId::from_parts(0, 2),
        current_revision: StateRevision::new(9),
        legacy_command_revision: Some(StateRevision::new(4)),
        effects: Vec::new(),
        superseded_effects: Vec::new(),
    })
    .expect("plan duplicate");
    let (_, _, mutation, facts, effects, commands, disposition) = plan.into_parts();
    assert_eq!(
        mutation,
        ListeningResetMutation::Duplicate {
            committed_revision: StateRevision::new(4)
        }
    );
    assert_eq!(disposition, RequestDisposition::Duplicate);
    assert_eq!(facts.len(), 1);
    assert!(effects.is_empty());
    assert!(commands.is_empty());
}
