use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DomainTransitionKind, DurableExternalEffectRequest, DurableInternalCommandRequest,
    LibraryFeedTransition, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

pub struct FeedDiscoveryRecoveryInput {
    pub recovery_id: CommandId,
    pub subject: ActivitySubject,
    pub episode_id: Option<EpisodeId>,
    pub current_revision: StateRevision,
    pub state_changes: bool,
}

pub type FeedDiscoveryRecoveryPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_feed_discovery_recovery(
    input: FeedDiscoveryRecoveryInput,
) -> Result<FeedDiscoveryRecoveryPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.recovery_id);
    let transaction_id = identity.transaction_id();
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: None,
        host_request_id: None,
        actor: ActivityActor::Recovery,
        origin: ActivityOrigin::Recovery,
        subject: input.subject,
        episode_id: input.episode_id,
        fact,
    };
    let mut tail = Vec::new();
    if input.state_changes {
        tail.push(base(
            1,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::LibraryFeed(
                    LibraryFeedTransition::FeedDiscoveryStateChanged,
                ),
                previous_revision: input.current_revision,
                committed_revision: StateRevision::new(
                    input
                        .current_revision
                        .value
                        .checked_add(1)
                        .ok_or(TransitionPlanError::RevisionExhausted)?,
                ),
            },
        ));
    }
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: if input.state_changes {
                        RequestDisposition::Accepted
                    } else {
                        RequestDisposition::NoSemanticChange
                    },
                },
            ),
            tail,
        ),
        Vec::new(),
        Vec::new(),
    )
}
