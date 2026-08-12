use std::collections::HashSet;

use pod0_domain::{
    ActivityCorrelationId, ActivityId, AgentTurnId, CategoryId, CommandId, EpisodeId,
    InternalCommandId, StateRevision,
};

use crate::{
    ActivityFact, ActivitySubject, AgentArtifactHandoffActivityInput, AgentArtifactMutation,
    AgentToolCompletion, RequestDisposition, RequestRejectionReason, UserArtifactTransition,
    plan_agent_artifact_handoff,
};

fn input(disposition: RequestDisposition) -> AgentArtifactHandoffActivityInput {
    let accepted = disposition == RequestDisposition::Accepted;
    AgentArtifactHandoffActivityInput {
        internal_command_id: InternalCommandId::from_parts(1, 1),
        authorizing_activity_id: ActivityId::from_parts(2, 1),
        correlation_id: ActivityCorrelationId::from_parts(3, 1),
        turn_id: AgentTurnId::from_parts(4, 1),
        subject: ActivitySubject::Operation {
            command_id: CommandId::from_parts(5, 1),
        },
        episode_ids: (0..70)
            .map(|value| EpisodeId::from_parts(6, value))
            .collect(),
        transition: UserArtifactTransition::CategoryChanged,
        completion: if accepted {
            AgentToolCompletion::CategoryChanged {
                category_id: CategoryId::from_parts(7, 1),
            }
        } else {
            AgentToolCompletion::Failed { code: 1 }
        },
        current_revision: StateRevision::new(9),
        committed_revision: StateRevision::new(if accepted { 10 } else { 9 }),
        disposition,
    }
}

#[test]
fn accepted_agent_artifact_projects_every_episode_without_identity_collisions() {
    let plan = plan_agent_artifact_handoff(input(RequestDisposition::Accepted)).unwrap();
    let (_, _, mutation, facts, _, commands, _) = plan.into_parts();
    assert_eq!(mutation, AgentArtifactMutation::Apply);
    assert_eq!(facts.len(), 72);
    let ids = facts
        .iter()
        .map(|fact| fact.activity_id.into_bytes())
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), facts.len());
    assert_eq!(commands[0].authorizing_fact_index, 71);
}

#[test]
fn rejected_agent_artifact_is_consumed_and_completed_without_a_domain_transition() {
    let plan = plan_agent_artifact_handoff(input(RequestDisposition::Rejected {
        reason: RequestRejectionReason::MissingSubject,
    }))
    .unwrap();
    let (_, _, mutation, facts, _, commands, disposition) = plan.into_parts();
    assert_eq!(mutation, AgentArtifactMutation::None);
    assert!(matches!(disposition, RequestDisposition::Rejected { .. }));
    assert_eq!(facts.len(), 2);
    assert!(facts.iter().all(|fact| !matches!(fact.fact, ActivityFact::DomainTransition { .. })));
    assert_eq!(commands[0].authorizing_fact_index, 1);
}
