use pod0_domain::{CommandId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DomainTransitionKind, DurableEffectExecution,
    DurableExternalEffectRequest, DurableInternalCommandRequest, ExternalEffectKind,
    NonEmptyActivityFacts, RecallKnowledgeTransition, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecallWorkflowEffect {
    Query(crate::DurableRecallQueryEffectRequest),
    Cutover(crate::DurableRecallIndexCutoverEffectRequest),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecallObservationActivityInput {
    pub command_id: CommandId,
    pub request_id: pod0_domain::HostRequestId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub intent_id: pod0_domain::EffectIntentId,
    pub attempt_id: pod0_domain::EffectAttemptId,
    pub authorizing_activity_id: pod0_domain::ActivityId,
    pub correlation_id: pod0_domain::ActivityCorrelationId,
    pub outcome: crate::EffectOutcome,
    pub transition: RecallKnowledgeTransition,
    pub next_request: Option<crate::DurableRecallQueryEffectRequest>,
}

pub fn plan_recall_observation(
    input: RecallObservationActivityInput,
) -> Result<RecallWorkflowActivityPlan, TransitionPlanError> {
    let identity = crate::EffectObservationActivityIdentity::new(input.attempt_id);
    let transaction_id = identity.transaction_id();
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: Some(input.command_id),
        host_request_id: Some(input.request_id),
        actor: ActivityActor::System,
        origin: ActivityOrigin::HostObservation,
        subject: ActivitySubject::Global,
        episode_id: None,
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
                kind: DomainTransitionKind::RecallKnowledge(input.transition),
                previous_revision: input.current_revision,
                committed_revision: input.committed_revision,
            },
        ),
    ];
    let effects = input
        .next_request
        .map(|request| {
            let intent_id = identity.effect_intent_id(0);
            tail.push(base(
                3,
                ActivityFact::EffectAuthorized {
                    intent_id,
                    kind: ExternalEffectKind::RecallProvider,
                },
            ));
            AuthorizedExternalEffect {
                intent_id,
                authorizing_fact_index: 3,
                request: DurableExternalEffectRequest {
                    kind: ExternalEffectKind::RecallProvider,
                    subject: ActivitySubject::Global,
                    episode_id: None,
                    not_before: None,
                    deadline_at: Some(request.deadline_at),
                    execution: DurableEffectExecution::RecallQuery { request },
                },
            }
        })
        .into_iter()
        .collect();
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecallWorkflowActivityInput {
    pub command_id: CommandId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub disposition: RequestDisposition,
    pub transition: RecallKnowledgeTransition,
    pub effect: Option<RecallWorkflowEffect>,
}

pub type RecallWorkflowActivityPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_recall_cutover_finalization(
    command_id: CommandId,
    current_revision: StateRevision,
    committed_revision: StateRevision,
) -> Result<RecallWorkflowActivityPlan, TransitionPlanError> {
    if committed_revision.value <= current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let identity = crate::CommandActivityIdentity::new(command_id);
    let transaction_id = identity.transaction_id();
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(command_id),
        host_request_id: None,
        actor: ActivityActor::System,
        origin: ActivityOrigin::InternalCommand,
        subject: ActivitySubject::Global,
        episode_id: None,
        fact,
    };
    TransitionPlan::new(
        transaction_id,
        current_revision,
        (),
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: RequestDisposition::Accepted,
                },
            ),
            vec![base(
                1,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::RecallKnowledge(
                        RecallKnowledgeTransition::IndexCutoverChanged,
                    ),
                    previous_revision: current_revision,
                    committed_revision,
                },
            )],
        ),
        Vec::new(),
        Vec::new(),
    )
}

pub fn plan_recall_workflow_activity(
    input: RecallWorkflowActivityInput,
) -> Result<RecallWorkflowActivityPlan, TransitionPlanError> {
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
        subject: ActivitySubject::Global,
        episode_id: None,
        fact,
    };
    let accepted = input.disposition == RequestDisposition::Accepted;
    if accepted && input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let head = base(
        0,
        ActivityFact::RequestDisposition {
            disposition: input.disposition,
        },
    );
    let mut tail = if accepted {
        vec![base(
            1,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::RecallKnowledge(input.transition),
                previous_revision: input.current_revision,
                committed_revision: input.committed_revision,
            },
        )]
    } else {
        Vec::new()
    };
    let effects = input
        .effect
        .map(|effect| {
            let intent_id = identity.effect_intent_id(0);
            tail.push(base(
                2,
                ActivityFact::EffectAuthorized {
                    intent_id,
                    kind: ExternalEffectKind::RecallProvider,
                },
            ));
            let (deadline_at, execution) = match effect {
                RecallWorkflowEffect::Query(request) => (
                    request.deadline_at,
                    DurableEffectExecution::RecallQuery { request },
                ),
                RecallWorkflowEffect::Cutover(request) => (
                    request.deadline_at,
                    DurableEffectExecution::RecallIndexCutover { request },
                ),
            };
            AuthorizedExternalEffect {
                intent_id,
                authorizing_fact_index: 2,
                request: DurableExternalEffectRequest {
                    kind: ExternalEffectKind::RecallProvider,
                    subject: ActivitySubject::Global,
                    episode_id: None,
                    not_before: None,
                    deadline_at: Some(deadline_at),
                    execution,
                },
            }
        })
        .into_iter()
        .collect();
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        NonEmptyActivityFacts::from_head_and_tail(head, tail),
        effects,
        Vec::new(),
    )
}
