use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EffectAttemptId, EffectIntentId, HostRequestId,
    StateRevision,
};
use sha2::{Digest as _, Sha256};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DurableEffectExecution, DurableExternalEffectRequest,
    DurableHostCancellationEffectRequest, DurableInternalCommandRequest, EffectOutcome,
    ExternalEffectKind, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CancellationEffectTarget {
    pub subject: ActivitySubject,
    pub episode_id: Option<pod0_domain::EpisodeId>,
    pub host_request_id: HostRequestId,
    pub cancellation_id: pod0_domain::CancellationId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CancellationActivityInput {
    pub command_id: CommandId,
    pub current_revision: StateRevision,
    pub targets: Vec<CancellationEffectTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedCancellationAuthorization {
    pub intent_id: EffectIntentId,
    pub request_id: HostRequestId,
    pub effect: AuthorizedExternalEffect<DurableExternalEffectRequest>,
}

pub fn prepare_cancellation_authorization(
    command_id: CommandId,
    current_revision: StateRevision,
    ordinal: u32,
    authorizing_fact_index: usize,
    target: CancellationEffectTarget,
) -> PreparedCancellationAuthorization {
    let (intent_id, request_id) = cancellation_effect_identity(command_id, ordinal);
    PreparedCancellationAuthorization {
        intent_id,
        request_id,
        effect: AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::Cancellation,
                subject: target.subject,
                episode_id: target.episode_id,
                not_before: None,
                deadline_at: None,
                execution: DurableEffectExecution::Cancellation {
                    request: DurableHostCancellationEffectRequest {
                        request_id,
                        command_id,
                        cancellation_id: target.cancellation_id,
                        issued_revision: current_revision,
                        target_request_id: target.host_request_id,
                    },
                },
            },
        },
    }
}

pub type CancellationActivityPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_cancellation_activity(
    input: CancellationActivityInput,
) -> Result<CancellationActivityPlan, TransitionPlanError> {
    if input.targets.len() > 200 {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let disposition = if input.targets.is_empty() {
        RequestDisposition::NoSemanticChange
    } else {
        RequestDisposition::Accepted
    };
    let head = ActivityFactDraft {
        activity_id: identity.fact_id(0),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::System,
        origin: ActivityOrigin::UserInterface,
        subject: ActivitySubject::Operation {
            command_id: input.command_id,
        },
        episode_id: None,
        fact: ActivityFact::RequestDisposition { disposition },
    };
    let mut tail = Vec::with_capacity(input.targets.len().saturating_mul(2));
    let mut effects = Vec::with_capacity(input.targets.len());
    for (index, target) in input.targets.into_iter().enumerate() {
        let ordinal =
            u32::try_from(index).map_err(|_| TransitionPlanError::InvalidEffectAuthorization)?;
        let superseded_index = tail.len() + 1;
        tail.push(ActivityFactDraft {
            activity_id: identity.fact_id_wide(ordinal.saturating_mul(2).saturating_add(1)),
            transaction_id,
            correlation_id: identity.correlation_id(),
            caused_by_activity_id: None,
            command_id: Some(input.command_id),
            host_request_id: Some(target.host_request_id),
            actor: ActivityActor::System,
            origin: ActivityOrigin::UserInterface,
            subject: target.subject,
            episode_id: target.episode_id,
            fact: ActivityFact::RecoveryTransition {
                outcome: EffectOutcome::Superseded,
            },
        });
        let authorization = prepare_cancellation_authorization(
            input.command_id,
            input.current_revision,
            ordinal,
            superseded_index + 1,
            target,
        );
        tail.push(ActivityFactDraft {
            activity_id: identity.fact_id_wide(ordinal.saturating_mul(2).saturating_add(2)),
            transaction_id,
            correlation_id: identity.correlation_id(),
            caused_by_activity_id: None,
            command_id: Some(input.command_id),
            host_request_id: Some(authorization.request_id),
            actor: ActivityActor::System,
            origin: ActivityOrigin::UserInterface,
            subject: target.subject,
            episode_id: target.episode_id,
            fact: ActivityFact::EffectAuthorized {
                intent_id: authorization.intent_id,
                kind: ExternalEffectKind::Cancellation,
            },
        });
        effects.push(authorization.effect);
    }
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        NonEmptyActivityFacts::from_head_and_tail(head, tail),
        effects,
        Vec::new(),
    )
}

fn cancellation_effect_identity(
    command_id: CommandId,
    ordinal: u32,
) -> (EffectIntentId, HostRequestId) {
    let derive = |label: &[u8]| {
        let mut hash = Sha256::new();
        hash.update(label);
        hash.update(command_id.into_bytes());
        hash.update(ordinal.to_be_bytes());
        <[u8; 16]>::try_from(&hash.finalize()[..16]).expect("digest prefix")
    };
    (
        EffectIntentId::from_bytes(derive(b"pod0/cancellation/intent/v1\0")),
        HostRequestId::from_bytes(derive(b"pod0/cancellation/request/v1\0")),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CancellationObservationActivityInput {
    pub attempt_id: EffectAttemptId,
    pub intent_id: EffectIntentId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub subject: ActivitySubject,
    pub episode_id: Option<pod0_domain::EpisodeId>,
    pub request: DurableHostCancellationEffectRequest,
    pub outcome: EffectOutcome,
}

pub fn plan_cancellation_observation(
    input: CancellationObservationActivityInput,
) -> Result<CancellationActivityPlan, TransitionPlanError> {
    let identity = crate::EffectObservationActivityIdentity::new(input.attempt_id);
    let transaction_id = identity.transaction_id();
    let fact = |ordinal, value| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: Some(input.request.command_id),
        host_request_id: Some(input.request.request_id),
        actor: ActivityActor::System,
        origin: ActivityOrigin::HostObservation,
        subject: input.subject,
        episode_id: input.episode_id,
        fact: value,
    };
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
                ActivityFact::EffectObserved {
                    intent_id: input.intent_id,
                    attempt_id: input.attempt_id,
                    outcome: input.outcome,
                },
            )],
        ),
        Vec::new(),
        Vec::new(),
    )
}
