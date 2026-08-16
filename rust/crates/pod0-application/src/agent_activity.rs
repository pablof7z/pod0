use pod0_domain::{
    ActivityCorrelationId, ActivityId, AgentTurnId, CommandId, EffectAttemptId, EffectIntentId,
    EpisodeId, HostRequestId, StateRevision,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AgentPublicationTransition, AuthorizedExternalEffect, DomainTransitionKind,
    DurableExternalEffectRequest, DurableInternalCommandRequest, EffectObservationActivityIdentity,
    EffectOutcome, ExternalEffectKind, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTurnStartActivityInput {
    pub command_id: CommandId,
    pub turn_id: AgentTurnId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub legacy_replay: bool,
    pub model: crate::DurableAgentModelEffectRequest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentEffectObservationActivityInput {
    pub command_id: CommandId,
    pub request_id: HostRequestId,
    pub turn_id: AgentTurnId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub intent_id: EffectIntentId,
    pub attempt_id: EffectAttemptId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub episode_id: Option<EpisodeId>,
    pub outcome: EffectOutcome,
    pub transition: AgentPublicationTransition,
    pub next_authorization: Option<AgentEffectAuthorization>,
    pub advance_turn: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentEffectAuthorization {
    Model(crate::DurableAgentModelEffectRequest),
    Approval(crate::DurableAgentApprovalEffectRequest),
    Capability(crate::DurableAgentCapabilityEffectRequest),
}

impl AgentEffectAuthorization {
    fn into_parts(self) -> (ExternalEffectKind, crate::DurableEffectExecution) {
        match self {
            Self::Model(request) => (
                ExternalEffectKind::AgentProvider,
                crate::DurableEffectExecution::AgentModel { request },
            ),
            Self::Approval(request) => (
                ExternalEffectKind::AgentApproval,
                crate::DurableEffectExecution::AgentApproval { request },
            ),
            Self::Capability(request) => (
                ExternalEffectKind::AgentCapability,
                crate::DurableEffectExecution::AgentCapability { request },
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplyAgentEffectObservation;

pub type AgentEffectObservationPlan = TransitionPlan<
    ApplyAgentEffectObservation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_agent_effect_observation(
    input: AgentEffectObservationActivityInput,
) -> Result<AgentEffectObservationPlan, TransitionPlanError> {
    if input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let identity = EffectObservationActivityIdentity::new(input.attempt_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::AgentTurn {
        turn_id: input.turn_id,
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: Some(input.command_id),
        host_request_id: Some(input.request_id),
        actor: ActivityActor::System,
        origin: ActivityOrigin::HostObservation,
        subject,
        episode_id: input.episode_id,
        fact,
    };
    let mut tail = vec![
        base(
            1,
            ActivityFact::EffectObserved {
                intent_id: input.intent_id,
                attempt_id: input.attempt_id,
                outcome: input.outcome,
            },
        ),
        base(
            2,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::AgentPublication(input.transition),
                previous_revision: input.current_revision,
                committed_revision: input.committed_revision,
            },
        ),
    ];
    let effects = input
        .next_authorization
        .map_or_else(Vec::new, |authorization| {
            let (kind, execution) = authorization.into_parts();
            let intent_id = identity.effect_intent_id(0);
            tail.push(base(3, ActivityFact::EffectAuthorized { intent_id, kind }));
            vec![AuthorizedExternalEffect {
                intent_id,
                authorizing_fact_index: 3,
                request: DurableExternalEffectRequest {
                    kind,
                    subject,
                    episode_id: input.episode_id,
                    not_before: None,
                    deadline_at: match &execution {
                        crate::DurableEffectExecution::AgentModel { request } => {
                            request.deadline_at
                        }
                        crate::DurableEffectExecution::AgentApproval { request } => {
                            request.deadline_at
                        }
                        crate::DurableEffectExecution::AgentCapability { request } => {
                            request.deadline_at
                        }
                        _ => unreachable!("agent authorization is exact"),
                    },
                    execution,
                },
            }]
        });
    let commands = if input.advance_turn {
        let internal_command_id = identity.internal_command_id(0);
        let index = tail.len() + 1;
        tail.push(base(
            u8::try_from(index).expect("bounded agent fact count"),
            ActivityFact::InternalCommandAuthorized {
                internal_command_id,
                target: crate::ActivityDomain::AgentPublication,
            },
        ));
        vec![crate::AuthorizedInternalCommand {
            internal_command_id,
            authorizing_fact_index: index,
            command: DurableInternalCommandRequest {
                kind: crate::InternalCommandKind::AdvanceAgentTurn {
                    turn_id: input.turn_id,
                },
                target: crate::ActivityDomain::AgentPublication,
                subject,
                episode_id: None,
            },
        }]
    } else {
        Vec::new()
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        ApplyAgentEffectObservation,
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: RequestDisposition::Accepted,
                },
            ),
            tail,
        ),
        effects,
        commands,
    )
}

include!("agent_host_failure.rs");

#[path = "agent_cancellation_activity.rs"]
mod cancellation;
pub use cancellation::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentTurnStartMutation {
    Start,
    LegacyDuplicate,
}

pub type AgentTurnStartPlan = TransitionPlan<
    AgentTurnStartMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_agent_turn_start(
    input: AgentTurnStartActivityInput,
) -> Result<AgentTurnStartPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::AgentTurn {
        turn_id: input.turn_id,
    };
    let disposition = if input.legacy_replay {
        RequestDisposition::Duplicate
    } else {
        RequestDisposition::Accepted
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject,
        episode_id: None,
        fact,
    };
    if input.legacy_replay {
        return TransitionPlan::new(
            transaction_id,
            input.current_revision,
            AgentTurnStartMutation::LegacyDuplicate,
            NonEmptyActivityFacts::new(base(0, ActivityFact::RequestDisposition { disposition })),
            Vec::new(),
            Vec::new(),
        );
    }
    if input.committed_revision.value != input.current_revision.value.saturating_add(1) {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let intent_id = identity.effect_intent_id(0);
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        AgentTurnStartMutation::Start,
        NonEmptyActivityFacts::from_head_and_tail(
            base(0, ActivityFact::RequestDisposition { disposition }),
            vec![
                base(
                    1,
                    ActivityFact::DomainTransition {
                        kind: DomainTransitionKind::AgentPublication(
                            AgentPublicationTransition::TurnStateChanged,
                        ),
                        previous_revision: input.current_revision,
                        committed_revision: input.committed_revision,
                    },
                ),
                base(
                    2,
                    ActivityFact::EffectAuthorized {
                        intent_id,
                        kind: ExternalEffectKind::AgentProvider,
                    },
                ),
            ],
        ),
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: 2,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::AgentProvider,
                subject,
                episode_id: None,
                not_before: None,
                deadline_at: input.model.deadline_at,
                execution: crate::DurableEffectExecution::AgentModel {
                    request: input.model,
                },
            },
        }],
        Vec::new(),
    )
}
