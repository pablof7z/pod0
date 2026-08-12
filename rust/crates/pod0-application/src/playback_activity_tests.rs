use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityFact, DomainTransitionKind, PlaybackActivityInput, PlaybackTransition,
    RequestDisposition, plan_playback_activity,
};

#[test]
fn playback_command_has_one_disposition_and_typed_transition() {
    let episode_id = EpisodeId::from_parts(8, 1);
    let plan = plan_playback_activity(PlaybackActivityInput {
        command_id: CommandId::from_parts(8, 2),
        episode_id: Some(episode_id),
        current_revision: StateRevision::new(9),
        legacy_command_revision: None,
        transition: PlaybackTransition::RateChanged,
        internal_command: None,
    })
    .unwrap();
    let (_, _, _, facts, _, _, _) = plan.into_parts();
    assert!(matches!(
        facts.get(0).unwrap().fact,
        ActivityFact::RequestDisposition {
            disposition: RequestDisposition::Accepted
        }
    ));
    assert!(matches!(
        facts.get(1).unwrap().fact,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::Playback(PlaybackTransition::RateChanged),
            previous_revision: StateRevision { value: 9 },
            committed_revision: StateRevision { value: 10 },
        }
    ));
    assert_eq!(facts.get(1).unwrap().episode_id, Some(episode_id));
}
