use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DomainTransitionKind, DurableExternalEffectRequest, DurableInternalCommandRequest,
    LibraryFeedTransition, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryCommandMutation {
    Apply,
    RecordNoChange,
    Duplicate { committed_revision: StateRevision },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibraryCommandActivityInput {
    pub command_id: CommandId,
    pub subject: ActivitySubject,
    pub episode_id: Option<EpisodeId>,
    pub current_revision: StateRevision,
    pub legacy_command_revision: Option<StateRevision>,
    pub transition: LibraryFeedTransition,
    pub semantic_change: bool,
}

pub type LibraryCommandActivityPlan = TransitionPlan<
    LibraryCommandMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_library_command(
    input: LibraryCommandActivityInput,
) -> Result<LibraryCommandActivityPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
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
        subject: input.subject,
        episode_id: input.episode_id,
        fact,
    };
    let head = base(0, ActivityFact::RequestDisposition { disposition });
    let (mutation, facts) = match disposition {
        RequestDisposition::Accepted => {
            let committed_revision = StateRevision::new(
                input
                    .current_revision
                    .value
                    .checked_add(1)
                    .ok_or(TransitionPlanError::RevisionExhausted)?,
            );
            (
                LibraryCommandMutation::Apply,
                NonEmptyActivityFacts::from_head_and_tail(
                    head,
                    vec![base(
                        1,
                        ActivityFact::DomainTransition {
                            kind: DomainTransitionKind::LibraryFeed(input.transition),
                            previous_revision: input.current_revision,
                            committed_revision,
                        },
                    )],
                ),
            )
        }
        RequestDisposition::Duplicate => (
            LibraryCommandMutation::Duplicate {
                committed_revision: input
                    .legacy_command_revision
                    .expect("duplicate library command has committed revision"),
            },
            NonEmptyActivityFacts::new(head),
        ),
        _ => (
            LibraryCommandMutation::RecordNoChange,
            NonEmptyActivityFacts::new(head),
        ),
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        mutation,
        facts,
        Vec::new(),
        Vec::new(),
    )
}
