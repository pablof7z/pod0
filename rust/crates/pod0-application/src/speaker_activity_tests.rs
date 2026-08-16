use pod0_domain::{CommandId, SpeakerEntityId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityOrigin, ActivitySubject, RequestDisposition,
    RequestRejectionReason, SpeakerActivityInput, SpeakerMutation, UserArtifactTransition,
    plan_speaker_activity,
};

fn input(disposition: RequestDisposition) -> SpeakerActivityInput {
    SpeakerActivityInput {
        command_id: CommandId::from_parts(1, 2),
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject: ActivitySubject::SpeakerEntity {
            speaker_entity_id: SpeakerEntityId::from_parts(3, 4),
        },
        episode_id: None,
        current_revision: StateRevision::new(7),
        committed_revision: if disposition == RequestDisposition::Accepted {
            StateRevision::new(8)
        } else {
            StateRevision::new(7)
        },
        transition: UserArtifactTransition::SpeakerIdentityChanged,
        disposition,
    }
}

#[test]
fn accepted_speaker_command_has_a_typed_transition() {
    let plan = plan_speaker_activity(input(RequestDisposition::Accepted)).unwrap();
    let (_, expected, mutation, facts, effects, commands, disposition) = plan.into_parts();
    assert_eq!(expected, StateRevision::new(7));
    assert_eq!(mutation, SpeakerMutation::Apply);
    assert_eq!(disposition, RequestDisposition::Accepted);
    assert!(effects.is_empty());
    assert!(commands.is_empty());
    assert_eq!(facts.len(), 2);
    assert!(matches!(
        facts.get(1).unwrap().fact,
        ActivityFact::DomainTransition { .. }
    ));
}

#[test]
fn rejected_speaker_command_is_durable_without_a_mutation() {
    let plan = plan_speaker_activity(input(RequestDisposition::Rejected {
        reason: RequestRejectionReason::MissingSubject,
    }))
    .unwrap();
    let (_, _, mutation, facts, _, _, _) = plan.into_parts();
    assert_eq!(mutation, SpeakerMutation::None);
    assert_eq!(facts.len(), 1);
}
