use pod0_domain::{CommandId, StateRevision};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, AgentPublicationTransition, DomainTransitionKind,
    DurableExternalEffectRequest, DurableInternalCommandRequest, NonEmptyActivityFacts,
    RequestDisposition, ScheduledAgentActivityTransition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyBusinessCutoverDomain {
    AgentHistory,
    ScheduledAgent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyBusinessCutoverActivityInput {
    pub command_id: CommandId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub domain: LegacyBusinessCutoverDomain,
    pub disposition: RequestDisposition,
    pub authority_cutover: bool,
}

pub type LegacyBusinessCutoverActivityPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_legacy_business_cutover(
    input: LegacyBusinessCutoverActivityInput,
) -> Result<LegacyBusinessCutoverActivityPlan, TransitionPlanError> {
    let accepted = input.disposition == RequestDisposition::Accepted;
    if accepted && input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let (subject, domain, transition) = match input.domain {
        LegacyBusinessCutoverDomain::AgentHistory => (
            ActivitySubject::Global,
            ActivityDomain::AgentPublication,
            DomainTransitionKind::AgentPublication(AgentPublicationTransition::TurnStateChanged),
        ),
        LegacyBusinessCutoverDomain::ScheduledAgent => (
            ActivitySubject::Global,
            ActivityDomain::ScheduledAgent,
            DomainTransitionKind::ScheduledAgent(ScheduledAgentActivityTransition::TaskChanged),
        ),
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::Migration,
        origin: ActivityOrigin::Migration,
        subject,
        episode_id: None,
        fact,
    };
    let mut tail = Vec::new();
    if accepted {
        tail.push(base(
            1,
            ActivityFact::DomainTransition {
                kind: transition,
                previous_revision: input.current_revision,
                committed_revision: input.committed_revision,
            },
        ));
        if input.authority_cutover {
            tail.push(base(2, ActivityFact::AuthorityCutover { domain }));
        }
    }
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: input.disposition,
                },
            ),
            tail,
        ),
        Vec::new(),
        Vec::new(),
    )
}
