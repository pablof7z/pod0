use pod0_domain::{EffectAttemptId, EffectIntentId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DurableEffectExecution, DurableExternalEffectRequest,
    DurableInternalCommandRequest, DurableLifecycleEffectRequest,
    EffectObservationActivityIdentity, EffectOutcome, ExternalEffectKind, NonEmptyActivityFacts,
    RequestDisposition, TransitionPlan, TransitionPlanError,
};

pub struct LifecycleWakeAdmissionInput {
    pub request: DurableLifecycleEffectRequest,
    pub subject: ActivitySubject,
}

pub type LifecycleWakePlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_lifecycle_wake_admission(
    input: LifecycleWakeAdmissionInput,
) -> Result<LifecycleWakePlan, TransitionPlanError> {
    if input.request.wake_at.value < 0 || input.request.attempt == 0 {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let identity = crate::CommandActivityIdentity::new(pod0_domain::CommandId::from_bytes(
        input.request.request_id.into_bytes(),
    ));
    let transaction_id = identity.transaction_id();
    let fact = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.request.command_id),
        host_request_id: Some(input.request.request_id),
        actor: ActivityActor::System,
        origin: ActivityOrigin::Recovery,
        subject: input.subject,
        episode_id: subject_episode(input.subject),
        fact,
    };
    let intent_id = identity.effect_intent_id(0);
    TransitionPlan::new(
        transaction_id,
        StateRevision::INITIAL,
        (),
        NonEmptyActivityFacts::from_head_and_tail(
            fact(
                0,
                ActivityFact::RequestDisposition {
                    disposition: RequestDisposition::Accepted,
                },
            ),
            vec![fact(
                1,
                ActivityFact::EffectAuthorized {
                    intent_id,
                    kind: ExternalEffectKind::CoreWake,
                },
            )],
        ),
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: 1,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::CoreWake,
                subject: input.subject,
                episode_id: subject_episode(input.subject),
                not_before: None,
                deadline_at: None,
                execution: DurableEffectExecution::Lifecycle {
                    request: input.request,
                },
            },
        }],
        Vec::new(),
    )
}

pub struct LifecycleWakeObservationInput {
    pub identity_attempt_id: EffectAttemptId,
    pub effect_attempt_id: EffectAttemptId,
    pub intent_id: EffectIntentId,
    pub authorizing_activity_id: pod0_domain::ActivityId,
    pub correlation_id: pod0_domain::ActivityCorrelationId,
    pub request: DurableLifecycleEffectRequest,
    pub subject: ActivitySubject,
    pub outcome: EffectOutcome,
    pub retry: Option<DurableLifecycleEffectRequest>,
}

pub fn plan_lifecycle_wake_observation(
    input: LifecycleWakeObservationInput,
) -> Result<LifecycleWakePlan, TransitionPlanError> {
    if input.retry.as_ref().is_some_and(|retry| {
        retry.reason != input.request.reason
            || retry.cancellation_id != input.request.cancellation_id
            || retry.attempt != input.request.attempt.saturating_add(1)
    }) {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let identity = EffectObservationActivityIdentity::new(input.identity_attempt_id);
    let transaction_id = identity.transaction_id();
    let fact = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: Some(input.request.command_id),
        host_request_id: Some(input.request.request_id),
        actor: ActivityActor::System,
        origin: ActivityOrigin::HostObservation,
        subject: input.subject,
        episode_id: subject_episode(input.subject),
        fact,
    };
    let mut tail = vec![fact(
        1,
        ActivityFact::EffectObserved {
            intent_id: input.intent_id,
            attempt_id: input.effect_attempt_id,
            outcome: input.outcome,
        },
    )];
    let effects = input.retry.map_or_else(Vec::new, |request| {
        let intent_id = identity.effect_intent_id(0);
        tail.push(fact(
            2,
            ActivityFact::EffectAuthorized {
                intent_id,
                kind: ExternalEffectKind::CoreWake,
            },
        ));
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: 2,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::CoreWake,
                subject: input.subject,
                episode_id: subject_episode(input.subject),
                not_before: Some(request.wake_at),
                deadline_at: None,
                execution: DurableEffectExecution::Lifecycle { request },
            },
        }]
    });
    TransitionPlan::new(
        transaction_id,
        StateRevision::INITIAL,
        (),
        NonEmptyActivityFacts::from_head_and_tail(
            fact(
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

const fn subject_episode(subject: ActivitySubject) -> Option<pod0_domain::EpisodeId> {
    match subject {
        ActivitySubject::Episode { episode_id } => Some(episode_id),
        _ => None,
    }
}
