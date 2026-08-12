use pod0_domain::{
    ActivityCorrelationId, ActivityId, AgentTurnId, CommandId, EpisodeId, InternalCommandId,
    StateRevision,
};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, AgentToolCompletion, AuthorizedInternalCommand, DomainTransitionKind,
    DurableExternalEffectRequest, DurableInternalCommandRequest, InternalCommandActivityIdentity,
    NonEmptyActivityFacts, RequestDisposition, TransitionPlan, TransitionPlanError,
    UserArtifactTransition,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentArtifactHandoffActivityInput {
    pub internal_command_id: InternalCommandId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub turn_id: AgentTurnId,
    pub subject: ActivitySubject,
    pub episode_ids: Vec<EpisodeId>,
    pub transition: UserArtifactTransition,
    pub completion: AgentToolCompletion,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub disposition: RequestDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentArtifactMutation {
    Apply,
    None,
}

pub type AgentArtifactHandoffPlan = TransitionPlan<
    AgentArtifactMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_agent_artifact_handoff(
    mut input: AgentArtifactHandoffActivityInput,
) -> Result<AgentArtifactHandoffPlan, TransitionPlanError> {
    let accepted = input.disposition == RequestDisposition::Accepted;
    if accepted && input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    if !accepted && input.committed_revision != input.current_revision {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let identity = InternalCommandActivityIdentity::new(input.internal_command_id);
    let transaction_id = identity.transaction_id();
    let command_id = CommandId::from_bytes(input.internal_command_id.into_bytes());
    let mut episodes = Vec::with_capacity(input.episode_ids.len());
    for episode_id in input.episode_ids.drain(..) {
        if !episodes.contains(&episode_id) {
            episodes.push(episode_id);
        }
    }
    let base = |ordinal, episode_id, fact| ActivityFactDraft {
        activity_id: identity.fact_id_wide(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: Some(command_id),
        host_request_id: None,
        actor: ActivityActor::Agent,
        origin: ActivityOrigin::InternalCommand,
        subject: input.subject,
        episode_id,
        fact,
    };
    let completion_id = identity.internal_command_id(0);
    let transition_episodes = if episodes.is_empty() {
        vec![None]
    } else {
        episodes.iter().copied().map(Some).collect::<Vec<_>>()
    };
    let completion_ordinal = if accepted {
        u32::try_from(transition_episodes.len() + 1).expect("artifact fact count fits u32")
    } else {
        1
    };
    let completion_authorization = ActivityFactDraft {
        subject: ActivitySubject::AgentTurn {
            turn_id: input.turn_id,
        },
        ..base(
            completion_ordinal,
            episodes.first().copied(),
            ActivityFact::InternalCommandAuthorized {
                internal_command_id: completion_id,
                target: ActivityDomain::AgentPublication,
            },
        )
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        if accepted {
            AgentArtifactMutation::Apply
        } else {
            AgentArtifactMutation::None
        },
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                episodes.first().copied(),
                ActivityFact::RequestDisposition {
                    disposition: input.disposition,
                },
            ),
            if accepted {
                let mut facts = transition_episodes
                    .into_iter()
                    .enumerate()
                    .map(|(index, episode_id)| {
                        base(
                            u32::try_from(index + 1).expect("artifact fact count fits u32"),
                            episode_id,
                            ActivityFact::DomainTransition {
                                kind: DomainTransitionKind::UserArtifact(input.transition),
                                previous_revision: input.current_revision,
                                committed_revision: input.committed_revision,
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                facts.push(completion_authorization);
                facts
            } else {
                vec![completion_authorization]
            },
        ),
        Vec::new(),
        vec![AuthorizedInternalCommand {
            internal_command_id: completion_id,
            authorizing_fact_index: if accepted {
                transition_episodes_len(&episodes) + 1
            } else {
                1
            },
            command: DurableInternalCommandRequest {
                kind: crate::InternalCommandKind::CompleteAgentTool {
                    turn_id: input.turn_id,
                    completion: input.completion,
                },
                target: ActivityDomain::AgentPublication,
                subject: ActivitySubject::AgentTurn {
                    turn_id: input.turn_id,
                },
                episode_id: episodes.first().copied(),
            },
        }],
    )
}

fn transition_episodes_len(episodes: &[EpisodeId]) -> usize {
    episodes.len().max(1)
}
