use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EffectAttemptId, EffectIntentId, HostRequestId,
    StateRevision,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DomainTransitionKind, DurableEffectExecution,
    DurableExternalEffectRequest, DurableInternalCommandRequest,
    DurableLibraryNetworkEffectRequest, EffectObservationActivityIdentity, EffectOutcome,
    ExternalEffectKind, LibraryFeedTransition, NonEmptyActivityFacts, RequestDisposition,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryNetworkMutation {
    Apply,
    Duplicate { committed_revision: StateRevision },
}

pub type LibraryNetworkPlan = TransitionPlan<
    LibraryNetworkMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_library_network_admission(
    command_id: CommandId,
    current_revision: StateRevision,
    duplicate_revision: Option<StateRevision>,
    request: DurableLibraryNetworkEffectRequest,
) -> Result<LibraryNetworkPlan, TransitionPlanError> {
    if request.command_id != command_id {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let identity = crate::CommandActivityIdentity::new(command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Operation { command_id };
    let disposition = if duplicate_revision.is_some() {
        RequestDisposition::Duplicate
    } else {
        RequestDisposition::Accepted
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(command_id),
        host_request_id: None,
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject,
        episode_id: None,
        fact,
    };
    let head = base(0, ActivityFact::RequestDisposition { disposition });
    if let Some(committed_revision) = duplicate_revision {
        return TransitionPlan::new(
            transaction_id,
            current_revision,
            LibraryNetworkMutation::Duplicate { committed_revision },
            NonEmptyActivityFacts::new(head),
            Vec::new(),
            Vec::new(),
        );
    }
    let committed_revision = next_revision(current_revision)?;
    let intent_id = identity.effect_intent_id(0);
    let mut effect_fact = base(
        2,
        ActivityFact::EffectAuthorized {
            intent_id,
            kind: ExternalEffectKind::LibraryNetwork,
        },
    );
    effect_fact.host_request_id = Some(request.request_id);
    TransitionPlan::new(
        transaction_id,
        current_revision,
        LibraryNetworkMutation::Apply,
        NonEmptyActivityFacts::from_head_and_tail(
            head,
            vec![
                base(
                    1,
                    ActivityFact::DomainTransition {
                        kind: DomainTransitionKind::LibraryFeed(
                            LibraryFeedTransition::LibraryNetworkStateChanged,
                        ),
                        previous_revision: current_revision,
                        committed_revision,
                    },
                ),
                effect_fact,
            ],
        ),
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: 2,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::LibraryNetwork,
                subject,
                episode_id: None,
                not_before: None,
                deadline_at: request.deadline_at,
                execution: DurableEffectExecution::LibraryNetwork { request },
            },
        }],
        Vec::new(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn plan_library_network_observation(
    attempt_id: EffectAttemptId,
    intent_id: EffectIntentId,
    authorizing_activity_id: ActivityId,
    correlation_id: ActivityCorrelationId,
    command_id: CommandId,
    request_id: HostRequestId,
    episode_id: Option<pod0_domain::EpisodeId>,
    current_revision: StateRevision,
    outcome: EffectOutcome,
    next_effect: Option<DurableLibraryNetworkEffectRequest>,
) -> Result<LibraryNetworkPlan, TransitionPlanError> {
    let identity = EffectObservationActivityIdentity::new(attempt_id);
    let transaction_id = identity.transaction_id();
    let subject = episode_id.map_or(ActivitySubject::Operation { command_id }, |episode_id| {
        ActivitySubject::Episode { episode_id }
    });
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id,
        caused_by_activity_id: Some(authorizing_activity_id),
        command_id: Some(command_id),
        host_request_id: Some(request_id),
        actor: ActivityActor::System,
        origin: ActivityOrigin::HostObservation,
        subject,
        episode_id,
        fact,
    };
    let committed_revision = next_revision(current_revision)?;
    let mut tail = vec![
        base(
            1,
            ActivityFact::EffectObserved {
                intent_id,
                attempt_id,
                outcome,
            },
        ),
        base(
            2,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::LibraryFeed(
                    LibraryFeedTransition::LibraryNetworkStateChanged,
                ),
                previous_revision: current_revision,
                committed_revision,
            },
        ),
    ];
    let effects = next_effect.map_or_else(Vec::new, |request| {
        let next_intent = identity.effect_intent_id(0);
        tail.push(base(
            3,
            ActivityFact::EffectAuthorized {
                intent_id: next_intent,
                kind: ExternalEffectKind::LibraryNetwork,
            },
        ));
        vec![AuthorizedExternalEffect {
            intent_id: next_intent,
            authorizing_fact_index: 3,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::LibraryNetwork,
                subject,
                episode_id: None,
                not_before: None,
                deadline_at: request.deadline_at,
                execution: DurableEffectExecution::LibraryNetwork { request },
            },
        }]
    });
    TransitionPlan::new(
        transaction_id,
        current_revision,
        LibraryNetworkMutation::Apply,
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

fn next_revision(value: StateRevision) -> Result<StateRevision, TransitionPlanError> {
    value
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(TransitionPlanError::RevisionExhausted)
}
