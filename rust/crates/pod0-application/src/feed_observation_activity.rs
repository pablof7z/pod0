use pod0_domain::{
    CommandId, EffectAttemptId, EffectIntentId, HostRequestId, PodcastId, StateRevision,
};

use pod0_domain::{ActivityCorrelationId, ActivityId};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DomainTransitionKind, DurableEffectExecution,
    DurableExternalEffectRequest, DurableFeedEffectRequest, DurableInternalCommandRequest,
    EffectObservationActivityIdentity, EffectOutcome, ExternalEffectKind, LibraryFeedTransition,
    NonEmptyActivityFacts, RequestDisposition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedObservationMutation {
    Apply,
    RecordNoChange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedObservationActivityInput {
    pub identity_attempt_id: EffectAttemptId,
    pub effect_attempt_id: EffectAttemptId,
    pub intent_id: EffectIntentId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub command_id: CommandId,
    pub request_id: HostRequestId,
    pub podcast_id: PodcastId,
    pub current_revision: StateRevision,
    pub outcome: EffectOutcome,
    pub state_changes: bool,
    pub next_effect: Option<DurableFeedEffectRequest>,
}

pub type FeedObservationActivityPlan = TransitionPlan<
    FeedObservationMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_feed_observation(
    input: FeedObservationActivityInput,
) -> Result<FeedObservationActivityPlan, TransitionPlanError> {
    if !input.state_changes && input.next_effect.is_some() {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let identity = EffectObservationActivityIdentity::new(input.identity_attempt_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Podcast {
        podcast_id: input.podcast_id,
    };
    let disposition = if input.state_changes {
        RequestDisposition::Accepted
    } else {
        RequestDisposition::NoSemanticChange
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
    let mut tail = vec![base(
        1,
        ActivityFact::EffectObserved {
            intent_id: input.intent_id,
            attempt_id: input.effect_attempt_id,
            outcome: input.outcome,
        },
    )];
    if input.state_changes {
        let committed_revision = StateRevision::new(
            input
                .current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        tail.push(base(
            2,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::LibraryFeed(
                    LibraryFeedTransition::FeedFetchStateChanged,
                ),
                previous_revision: input.current_revision,
                committed_revision,
            },
        ));
    }
    let effects = input.next_effect.map_or_else(Vec::new, |request| {
        let intent_id = identity.effect_intent_id(0);
        let fact_index = tail.len() + 1;
        tail.push(base(
            u8::try_from(fact_index).expect("bounded feed observation facts"),
            ActivityFact::EffectAuthorized {
                intent_id,
                kind: ExternalEffectKind::FeedNetwork,
            },
        ));
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: fact_index,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::FeedNetwork,
                subject,
                episode_id: None,
                not_before: request.not_before,
                deadline_at: request.deadline_at,
                execution: DurableEffectExecution::Feed { request },
            },
        }]
    });
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        if input.state_changes {
            FeedObservationMutation::Apply
        } else {
            FeedObservationMutation::RecordNoChange
        },
        NonEmptyActivityFacts::from_head_and_tail(
            base(0, ActivityFact::RequestDisposition { disposition }),
            tail,
        ),
        effects,
        Vec::new(),
    )
}
