use pod0_domain::{CommandId, StateRevision};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, LibraryFeedTransition, NonEmptyActivityFacts,
    RequestDisposition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibraryFeedMigrationInput {
    pub migration_id: CommandId,
    pub current_revision: StateRevision,
    pub transition: LibraryFeedTransition,
    pub disposition: RequestDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryFeedMigrationMutation {
    Apply,
    None,
}

pub type LibraryFeedMigrationPlan = TransitionPlan<
    LibraryFeedMigrationMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_library_feed_migration(
    input: LibraryFeedMigrationInput,
) -> Result<LibraryFeedMigrationPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.migration_id);
    let transaction_id = identity.transaction_id();
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: None,
        host_request_id: None,
        actor: ActivityActor::Migration,
        origin: ActivityOrigin::Migration,
        subject: ActivitySubject::Global,
        episode_id: None,
        fact,
    };
    let accepted = input.disposition == RequestDisposition::Accepted;
    let mut tail = Vec::new();
    if accepted {
        let committed_revision = StateRevision::new(
            input
                .current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        tail.push(base(
            1,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::LibraryFeed(input.transition),
                previous_revision: input.current_revision,
                committed_revision,
            },
        ));
        tail.push(base(
            2,
            ActivityFact::AuthorityCutover {
                domain: ActivityDomain::LibraryFeed,
            },
        ));
    }
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        if accepted {
            LibraryFeedMigrationMutation::Apply
        } else {
            LibraryFeedMigrationMutation::None
        },
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
