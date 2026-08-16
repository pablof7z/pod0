use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EffectAttemptId, EffectIntentId, HostRequestId,
    PublicationId, StateRevision,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AgentPublicationTransition, AuthorizedExternalEffect, DomainTransitionKind,
    DurableEffectExecution, DurableExternalEffectRequest, DurableInternalCommandRequest,
    EffectObservationActivityIdentity, EffectOutcome, ExternalEffectKind, NonEmptyActivityFacts,
    Pod0PublicationDraft, RequestDisposition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationPrepareActivityInput {
    pub command_id: CommandId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub disposition: RequestDisposition,
    pub draft: Option<Pod0PublicationDraft>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplyPublicationPrepare;

pub type PublicationPreparePlan = TransitionPlan<
    ApplyPublicationPrepare,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_publication_prepare(
    input: PublicationPrepareActivityInput,
) -> Result<PublicationPreparePlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let publication_id = input.draft.as_ref().map(|draft| draft.publication_id);
    let subject = publication_id.map_or(ActivitySubject::Global, |publication_id| {
        ActivitySubject::Publication { publication_id }
    });
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
    if input.disposition == RequestDisposition::Accepted {
        if input.committed_revision.value <= input.current_revision.value || input.draft.is_none() {
            return Err(TransitionPlanError::DispositionRequiresTransition);
        }
    } else if input.committed_revision != input.current_revision || input.draft.is_some() {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let (tail, effects) = match input.draft {
        Some(draft) => {
            let intent_id = identity.effect_intent_id(0);
            (
                vec![
                    base(
                        1,
                        ActivityFact::DomainTransition {
                            kind: DomainTransitionKind::AgentPublication(
                                AgentPublicationTransition::PublicationStateChanged,
                            ),
                            previous_revision: input.current_revision,
                            committed_revision: input.committed_revision,
                        },
                    ),
                    base(
                        2,
                        ActivityFact::EffectAuthorized {
                            intent_id,
                            kind: ExternalEffectKind::Publication,
                        },
                    ),
                ],
                vec![AuthorizedExternalEffect {
                    intent_id,
                    authorizing_fact_index: 2,
                    request: DurableExternalEffectRequest {
                        kind: ExternalEffectKind::Publication,
                        subject,
                        episode_id: None,
                        not_before: None,
                        deadline_at: None,
                        execution: DurableEffectExecution::Publication { draft },
                    },
                }],
            )
        }
        None => (Vec::new(), Vec::new()),
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        ApplyPublicationPrepare,
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: input.disposition,
                },
            ),
            tail,
        ),
        effects,
        Vec::new(),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicationObservationActivityInput {
    pub request_id: HostRequestId,
    pub publication_id: PublicationId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub intent_id: EffectIntentId,
    pub observation_activity_id: EffectAttemptId,
    pub attempt_id: EffectAttemptId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub outcome: EffectOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplyPublicationObservation;

pub type PublicationObservationPlan = TransitionPlan<
    ApplyPublicationObservation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_publication_observation(
    input: PublicationObservationActivityInput,
) -> Result<PublicationObservationPlan, TransitionPlanError> {
    if input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let identity = EffectObservationActivityIdentity::new(input.observation_activity_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Publication {
        publication_id: input.publication_id,
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: Some(CommandId::from_bytes(input.request_id.into_bytes())),
        host_request_id: Some(input.request_id),
        actor: ActivityActor::System,
        origin: ActivityOrigin::HostObservation,
        subject,
        episode_id: None,
        fact,
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        ApplyPublicationObservation,
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: RequestDisposition::Accepted,
                },
            ),
            vec![
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
                        kind: DomainTransitionKind::AgentPublication(
                            AgentPublicationTransition::PublicationStateChanged,
                        ),
                        previous_revision: input.current_revision,
                        committed_revision: input.committed_revision,
                    },
                ),
            ],
        ),
        Vec::new(),
        Vec::new(),
    )
}
