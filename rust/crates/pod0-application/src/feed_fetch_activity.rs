use pod0_domain::{CommandId, PodcastId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DomainTransitionKind, DurableEffectExecution,
    DurableExternalEffectRequest, DurableFeedEffectRequest, DurableInternalCommandRequest,
    ExternalEffectKind, LibraryFeedTransition, NonEmptyActivityFacts, RequestDisposition,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedFetchActivityMutation {
    Apply,
    RecordNoChange,
    Duplicate { committed_revision: StateRevision },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedFetchActivityInput {
    pub command_id: CommandId,
    pub podcast_id: PodcastId,
    pub current_revision: StateRevision,
    pub legacy_command_revision: Option<StateRevision>,
    pub semantic_change: bool,
    pub effect: Option<DurableFeedEffectRequest>,
}

pub type FeedFetchActivityPlan = TransitionPlan<
    FeedFetchActivityMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_feed_fetch(
    input: FeedFetchActivityInput,
) -> Result<FeedFetchActivityPlan, TransitionPlanError> {
    if input.effect.as_ref().is_some_and(|effect| {
        effect.command_id != input.command_id
            || effect.podcast_id() != input.podcast_id
            || effect.episode_id().is_some()
    }) || (!input.semantic_change && input.effect.is_some())
    {
        return Err(TransitionPlanError::InvalidEffectAuthorization);
    }
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Podcast {
        podcast_id: input.podcast_id,
    };
    let disposition = if input.legacy_command_revision.is_some() {
        RequestDisposition::Duplicate
    } else if input.semantic_change {
        RequestDisposition::Accepted
    } else {
        RequestDisposition::NoSemanticChange
    };
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
    let head = base(0, ActivityFact::RequestDisposition { disposition });
    let mut effects = Vec::new();
    let (mutation, facts) = match disposition {
        RequestDisposition::Accepted => {
            let committed_revision = StateRevision::new(
                input
                    .current_revision
                    .value
                    .checked_add(1)
                    .ok_or(TransitionPlanError::RevisionExhausted)?,
            );
            let mut tail = vec![base(
                1,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::LibraryFeed(
                        LibraryFeedTransition::FeedFetchStateChanged,
                    ),
                    previous_revision: input.current_revision,
                    committed_revision,
                },
            )];
            if let Some(request) = input.effect {
                let intent_id = identity.effect_intent_id(0);
                let mut fact = base(
                    2,
                    ActivityFact::EffectAuthorized {
                        intent_id,
                        kind: ExternalEffectKind::FeedNetwork,
                    },
                );
                fact.host_request_id = Some(request.request_id);
                tail.push(fact);
                effects.push(AuthorizedExternalEffect {
                    intent_id,
                    authorizing_fact_index: 2,
                    request: DurableExternalEffectRequest {
                        kind: ExternalEffectKind::FeedNetwork,
                        subject,
                        episode_id: None,
                        not_before: None,
                        deadline_at: request.deadline_at,
                        execution: DurableEffectExecution::Feed { request },
                    },
                });
            }
            (
                FeedFetchActivityMutation::Apply,
                NonEmptyActivityFacts::from_head_and_tail(head, tail),
            )
        }
        RequestDisposition::Duplicate => (
            FeedFetchActivityMutation::Duplicate {
                committed_revision: input
                    .legacy_command_revision
                    .expect("duplicate feed command has a committed revision"),
            },
            NonEmptyActivityFacts::new(head),
        ),
        _ => (
            FeedFetchActivityMutation::RecordNoChange,
            NonEmptyActivityFacts::new(head),
        ),
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        mutation,
        facts,
        effects,
        Vec::new(),
    )
}
