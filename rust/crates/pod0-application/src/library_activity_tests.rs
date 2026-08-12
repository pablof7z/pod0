use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityFact, EpisodeStarredMutation, EpisodeStarredState, RequestDisposition,
    plan_episode_starred,
};

#[test]
fn star_transition_is_pure_semantic_and_revisioned() {
    let plan = plan_episode_starred(
        CommandId::from_parts(1, 2),
        EpisodeStarredState {
            episode_id: EpisodeId::from_parts(3, 4),
            starred: false,
            revision: StateRevision::new(7),
            legacy_command_revision: None,
        },
        true,
    )
    .unwrap();
    assert_eq!(plan.disposition(), RequestDisposition::Accepted);
    let (_, expected, mutation, facts, effects, commands, _) = plan.into_parts();
    assert_eq!(expected, StateRevision::new(7));
    assert_eq!(
        mutation,
        EpisodeStarredMutation::Set {
            episode_id: EpisodeId::from_parts(3, 4),
            starred: true,
        }
    );
    assert!(matches!(
        facts.get(1).unwrap().fact,
        ActivityFact::DomainTransition { .. }
    ));
    assert!(effects.is_empty());
    assert!(commands.is_empty());
}

#[test]
fn unchanged_and_legacy_replay_have_dispositions_without_transition_facts() {
    for (legacy, expected) in [
        (None, RequestDisposition::NoSemanticChange),
        (Some(StateRevision::new(6)), RequestDisposition::Duplicate),
    ] {
        let plan = plan_episode_starred(
            CommandId::from_parts(1, 2),
            EpisodeStarredState {
                episode_id: EpisodeId::from_parts(3, 4),
                starred: true,
                revision: StateRevision::new(7),
                legacy_command_revision: legacy,
            },
            true,
        )
        .unwrap();
        assert_eq!(plan.disposition(), expected);
        assert_eq!(plan.into_parts().3.len(), 1);
    }
}
