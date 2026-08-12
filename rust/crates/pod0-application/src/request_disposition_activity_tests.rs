use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityOrigin, ActivitySubject, RequestDisposition,
    RequestDispositionActivityInput, RequestRejectionReason, plan_request_disposition,
};

#[test]
fn rejected_boundary_request_has_one_bounded_fact_and_no_fake_transition() {
    let episode_id = EpisodeId::from_parts(3, 4);
    let plan = plan_request_disposition(RequestDispositionActivityInput {
        command_id: CommandId::from_parts(1, 2),
        subject: ActivitySubject::Episode { episode_id },
        episode_id: Some(episode_id),
        current_revision: StateRevision::new(5),
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        disposition: RequestDisposition::Rejected {
            reason: RequestRejectionReason::MissingSubject,
        },
    })
    .unwrap();
    let (_, _, _, facts, effects, commands, _) = plan.into_parts();
    assert_eq!(facts.len(), 1);
    assert!(effects.is_empty());
    assert!(commands.is_empty());
}
