use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EffectAttemptId, EffectIntentId, EpisodeId,
    HostRequestId, StateRevision,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DomainTransitionKind, DurableEffectExecution,
    DurableExternalEffectRequest, DurableFeedEffectRequest, DurableInternalCommandRequest,
    EffectObservationActivityIdentity, EffectOutcome, ExternalEffectKind, LibraryFeedTransition,
    NonEmptyActivityFacts, RequestDisposition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedNotificationObservationMutation {
    Apply,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedNotificationObservationInput {
    pub identity_attempt_id: EffectAttemptId,
    pub effect_attempt_id: EffectAttemptId,
    pub intent_id: EffectIntentId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub command_id: CommandId,
    pub request_id: HostRequestId,
    pub episode_id: EpisodeId,
    pub current_revision: StateRevision,
    pub outcome: EffectOutcome,
    pub retry_effect: Option<DurableFeedEffectRequest>,
}

pub type FeedNotificationObservationPlan = TransitionPlan<
    FeedNotificationObservationMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_feed_notification_observation(
    input: FeedNotificationObservationInput,
) -> Result<FeedNotificationObservationPlan, TransitionPlanError> {
    if input
        .retry_effect
        .as_ref()
        .is_some_and(|effect| effect.episode_id() != Some(input.episode_id))
    {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let identity = EffectObservationActivityIdentity::new(input.identity_attempt_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
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
        episode_id: Some(input.episode_id),
        fact,
    };
    let committed_revision = StateRevision::new(
        input
            .current_revision
            .value
            .checked_add(1)
            .ok_or(TransitionPlanError::RevisionExhausted)?,
    );
    let mut tail = vec![
        base(
            1,
            ActivityFact::EffectObserved {
                intent_id: input.intent_id,
                attempt_id: input.effect_attempt_id,
                outcome: input.outcome,
            },
        ),
        base(
            2,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::LibraryFeed(
                    LibraryFeedTransition::NotificationDeliveryStateChanged,
                ),
                previous_revision: input.current_revision,
                committed_revision,
            },
        ),
    ];
    let effects = input.retry_effect.map_or_else(Vec::new, |request| {
        let intent_id = identity.effect_intent_id(0);
        tail.push(base(
            3,
            ActivityFact::EffectAuthorized {
                intent_id,
                kind: ExternalEffectKind::Notification,
            },
        ));
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: 3,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::Notification,
                subject,
                episode_id: Some(input.episode_id),
                not_before: request.not_before,
                deadline_at: request.deadline_at,
                execution: DurableEffectExecution::Feed { request },
            },
        }]
    });
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        FeedNotificationObservationMutation::Apply,
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
