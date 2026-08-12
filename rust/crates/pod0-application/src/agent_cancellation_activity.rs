use pod0_domain::{AgentTurnId, CommandId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AgentPublicationTransition, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentCancellationActivityInput {
    pub command_id: CommandId,
    pub turn_id: AgentTurnId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub disposition: RequestDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentCancellationMutation {
    Cancel,
    None,
}

pub type AgentCancellationPlan = TransitionPlan<
    AgentCancellationMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_agent_cancellation(
    input: AgentCancellationActivityInput,
) -> Result<AgentCancellationPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject: ActivitySubject::AgentTurn {
            turn_id: input.turn_id,
        },
        episode_id: None,
        fact,
    };
    let accepted = input.disposition == RequestDisposition::Accepted;
    if accepted && input.committed_revision.value != input.current_revision.value.saturating_add(1)
    {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let facts = if accepted {
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: input.disposition,
                },
            ),
            vec![base(
                1,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::AgentPublication(
                        AgentPublicationTransition::TurnStateChanged,
                    ),
                    previous_revision: input.current_revision,
                    committed_revision: input.committed_revision,
                },
            )],
        )
    } else {
        NonEmptyActivityFacts::new(base(
            0,
            ActivityFact::RequestDisposition {
                disposition: input.disposition,
            },
        ))
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        if accepted {
            AgentCancellationMutation::Cancel
        } else {
            AgentCancellationMutation::None
        },
        facts,
        Vec::new(),
        Vec::new(),
    )
}
