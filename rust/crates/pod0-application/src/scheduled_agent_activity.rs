use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EffectAttemptId, EffectIntentId, HostRequestId,
    ScheduledOccurrenceId, StateRevision,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, EffectObservationActivityIdentity, EffectOutcome,
    ExternalEffectKind, NonEmptyActivityFacts, RequestDisposition,
    ScheduledAgentActivityTransition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledEffectAuthorization {
    pub request: crate::DurableScheduledAgentEffectRequest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledCommandActivityInput {
    pub command_id: CommandId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub disposition: RequestDisposition,
    pub transitions: Vec<(ActivitySubject, ScheduledAgentActivityTransition)>,
    pub effects: Vec<ScheduledEffectAuthorization>,
    pub superseded_effects: Vec<ActivitySubject>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplyScheduledCommand;

pub type ScheduledCommandPlan = TransitionPlan<
    ApplyScheduledCommand,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_scheduled_command(
    input: ScheduledCommandActivityInput,
) -> Result<ScheduledCommandPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let correlation_id = identity.correlation_id();
    let base = |ordinal, subject, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id,
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::System,
        origin: ActivityOrigin::ScheduledWork,
        subject,
        episode_id: None,
        fact,
    };
    let changed = !matches!(
        input.disposition,
        RequestDisposition::Duplicate
            | RequestDisposition::Stale
            | RequestDisposition::AlreadyComplete
            | RequestDisposition::NoSemanticChange
            | RequestDisposition::Rejected { .. }
    );
    if changed && input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    if !changed && (!input.transitions.is_empty() || !input.effects.is_empty()) {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let mut tail = Vec::with_capacity(
        input.transitions.len() + input.effects.len() + input.superseded_effects.len(),
    );
    for (subject, transition) in input.transitions {
        let ordinal = u8::try_from(tail.len() + 1)
            .map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?;
        tail.push(base(
            ordinal,
            subject,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::ScheduledAgent(transition),
                previous_revision: input.current_revision,
                committed_revision: input.committed_revision,
            },
        ));
    }
    let mut effects = Vec::with_capacity(input.effects.len());
    for (effect_ordinal, effect) in input.effects.into_iter().enumerate() {
        let subject = ActivitySubject::ScheduledOccurrence {
            occurrence_id: effect.request.execution.occurrence_id,
        };
        let intent_id = identity.effect_intent_id(
            u8::try_from(effect_ordinal)
                .map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?,
        );
        let fact_index = tail.len() + 1;
        let ordinal = u8::try_from(fact_index)
            .map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?;
        tail.push(base(
            ordinal,
            subject,
            ActivityFact::EffectAuthorized {
                intent_id,
                kind: ExternalEffectKind::ScheduledAgentProvider,
            },
        ));
        effects.push(AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: fact_index,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::ScheduledAgentProvider,
                subject,
                episode_id: None,
                not_before: None,
                deadline_at: Some(effect.request.deadline_at),
                execution: crate::DurableEffectExecution::ScheduledAgent {
                    request: effect.request,
                },
            },
        });
    }
    for subject in input.superseded_effects {
        let ordinal = u8::try_from(tail.len() + 1)
            .map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?;
        tail.push(base(
            ordinal,
            subject,
            ActivityFact::RecoveryTransition {
                outcome: EffectOutcome::Superseded,
            },
        ));
    }
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        ApplyScheduledCommand,
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivitySubject::Global,
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
pub struct ScheduledObservationActivityInput {
    pub command_id: CommandId,
    pub request_id: HostRequestId,
    pub occurrence_id: ScheduledOccurrenceId,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub intent_id: EffectIntentId,
    /// A stable identity derived from the effect attempt and observation
    /// sequence. Scheduled provider effects may report accepted and terminal
    /// observations on the same lease, so the lease attempt alone is not a
    /// unique activity transaction identity.
    pub observation_activity_id: EffectAttemptId,
    pub attempt_id: EffectAttemptId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub outcome: EffectOutcome,
    pub transition: ScheduledAgentActivityTransition,
    pub disposition: RequestDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplyScheduledObservation;

pub type ScheduledObservationPlan = TransitionPlan<
    ApplyScheduledObservation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_scheduled_observation(
    input: ScheduledObservationActivityInput,
) -> Result<ScheduledObservationPlan, TransitionPlanError> {
    let applied = input.disposition == RequestDisposition::Accepted;
    if applied && input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    if !applied && input.committed_revision != input.current_revision {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let identity = EffectObservationActivityIdentity::new(input.observation_activity_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::ScheduledOccurrence {
        occurrence_id: input.occurrence_id,
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
        episode_id: None,
        fact,
    };
    let tail = if applied {
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
                    kind: DomainTransitionKind::ScheduledAgent(input.transition),
                    previous_revision: input.current_revision,
                    committed_revision: input.committed_revision,
                },
            ),
        ]
    } else {
        Vec::new()
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        ApplyScheduledObservation,
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
