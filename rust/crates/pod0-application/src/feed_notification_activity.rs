use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DomainTransitionKind, DurableEffectExecution,
    DurableExternalEffectRequest, DurableFeedEffectRequest, DurableInternalCommandRequest,
    ExternalEffectKind, LibraryFeedTransition, NonEmptyActivityFacts, RequestDisposition,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedNotificationAdmissionInput {
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub current_revision: StateRevision,
    pub effect: DurableFeedEffectRequest,
}

pub type FeedNotificationAdmissionPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_feed_notification_admission(
    input: FeedNotificationAdmissionInput,
) -> Result<FeedNotificationAdmissionPlan, TransitionPlanError> {
    if input.effect.command_id != input.command_id
        || input.effect.episode_id() != Some(input.episode_id)
    {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: Some(input.effect.request_id),
        actor: ActivityActor::System,
        origin: ActivityOrigin::AutomaticPolicy,
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
    let intent_id = identity.effect_intent_id(0);
    let facts = NonEmptyActivityFacts::from_head_and_tail(
        base(
            0,
            ActivityFact::RequestDisposition {
                disposition: RequestDisposition::Accepted,
            },
        ),
        vec![
            base(
                1,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::LibraryFeed(
                        LibraryFeedTransition::NotificationDeliveryStateChanged,
                    ),
                    previous_revision: input.current_revision,
                    committed_revision,
                },
            ),
            base(
                2,
                ActivityFact::EffectAuthorized {
                    intent_id,
                    kind: ExternalEffectKind::Notification,
                },
            ),
        ],
    );
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        facts,
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: 2,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::Notification,
                subject,
                episode_id: Some(input.episode_id),
                not_before: input.effect.not_before,
                deadline_at: input.effect.deadline_at,
                execution: DurableEffectExecution::Feed {
                    request: input.effect,
                },
            },
        }],
        Vec::new(),
    )
}
