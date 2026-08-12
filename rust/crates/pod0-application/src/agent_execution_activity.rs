use pod0_domain::{
    ActivityCorrelationId, ActivityId, AgentTurnId, CommandId, InternalCommandId, StateRevision,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AgentPublicationTransition, AuthorizedExternalEffect, AuthorizedInternalCommand,
    DomainTransitionKind, DurableExternalEffectRequest, DurableInternalCommandRequest,
    ExternalEffectKind, InternalCommandActivityIdentity, NonEmptyActivityFacts, RequestDisposition,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentExecutionActivityInput {
    pub internal_command_id: InternalCommandId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub turn_id: AgentTurnId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub continuation: AgentExecutionContinuation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentExecutionContinuation {
    None,
    NativeCapability,
    RustProjection,
    RustTool { target: crate::ActivityDomain },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BeginAgentExecution;

pub type AgentExecutionPlan = TransitionPlan<
    BeginAgentExecution,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_agent_execution(
    input: AgentExecutionActivityInput,
) -> Result<AgentExecutionPlan, TransitionPlanError> {
    if input.committed_revision.value != input.current_revision.value.saturating_add(1) {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let identity = InternalCommandActivityIdentity::new(input.internal_command_id);
    let transaction_id = identity.transaction_id();
    let command_id = CommandId::from_bytes(input.internal_command_id.into_bytes());
    let subject = ActivitySubject::AgentTurn {
        turn_id: input.turn_id,
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: Some(command_id),
        host_request_id: None,
        actor: ActivityActor::Agent,
        origin: ActivityOrigin::InternalCommand,
        subject,
        episode_id: None,
        fact,
    };
    let mut tail = vec![base(
        1,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::AgentPublication(
                AgentPublicationTransition::ToolStateChanged,
            ),
            previous_revision: input.current_revision,
            committed_revision: input.committed_revision,
        },
    )];
    let effects = if input.continuation == AgentExecutionContinuation::NativeCapability {
        let intent_id = identity.effect_intent_id(0);
        tail.push(base(
            2,
            ActivityFact::EffectAuthorized {
                intent_id,
                kind: ExternalEffectKind::AgentCapability,
            },
        ));
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: 2,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::AgentCapability,
                subject,
                episode_id: None,
                not_before: None,
                deadline_at: None,
            },
        }]
    } else {
        Vec::new()
    };
    let internal_target = match input.continuation {
        AgentExecutionContinuation::RustProjection => Some(crate::ActivityDomain::AgentPublication),
        AgentExecutionContinuation::RustTool { target } => Some(target),
        _ => None,
    };
    let commands = if let Some(target) = internal_target {
        let internal_command_id = identity.internal_command_id(0);
        tail.push(base(
            2,
            ActivityFact::InternalCommandAuthorized {
                internal_command_id,
                target,
            },
        ));
        vec![AuthorizedInternalCommand {
            internal_command_id,
            authorizing_fact_index: 2,
            command: DurableInternalCommandRequest {
                kind: match input.continuation {
                    AgentExecutionContinuation::RustProjection => {
                        crate::InternalCommandKind::ExecuteAgentProjection {
                            turn_id: input.turn_id,
                        }
                    }
                    AgentExecutionContinuation::RustTool { .. } => {
                        crate::InternalCommandKind::ExecuteAgentTool {
                            turn_id: input.turn_id,
                        }
                    }
                    _ => unreachable!("internal target requires internal continuation"),
                },
                target,
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
        BeginAgentExecution,
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

#[path = "agent_artifact_handoff_activity.rs"]
mod tool_handoff;
pub use tool_handoff::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentProjectionCompletionActivityInput {
    pub internal_command_id: InternalCommandId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub turn_id: AgentTurnId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub authorize_continuation_model: bool,
}

pub fn plan_agent_projection_completion(
    input: AgentProjectionCompletionActivityInput,
) -> Result<AgentExecutionPlan, TransitionPlanError> {
    if input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let identity = InternalCommandActivityIdentity::new(input.internal_command_id);
    let transaction_id = identity.transaction_id();
    let command_id = CommandId::from_bytes(input.internal_command_id.into_bytes());
    let subject = ActivitySubject::AgentTurn {
        turn_id: input.turn_id,
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: Some(command_id),
        host_request_id: None,
        actor: ActivityActor::Agent,
        origin: ActivityOrigin::InternalCommand,
        subject,
        episode_id: None,
        fact,
    };
    let mut tail = vec![base(
        1,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::AgentPublication(
                AgentPublicationTransition::ToolStateChanged,
            ),
            previous_revision: input.current_revision,
            committed_revision: input.committed_revision,
        },
    )];
    let effects = if input.authorize_continuation_model {
        let intent_id = identity.effect_intent_id(0);
        tail.push(base(
            2,
            ActivityFact::EffectAuthorized {
                intent_id,
                kind: ExternalEffectKind::AgentProvider,
            },
        ));
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: 2,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::AgentProvider,
                subject,
                episode_id: None,
                not_before: None,
                deadline_at: None,
            },
        }]
    } else {
        Vec::new()
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        BeginAgentExecution,
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
        Vec::new(),
    )
}
