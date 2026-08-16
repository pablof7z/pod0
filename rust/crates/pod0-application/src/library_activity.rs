use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DomainTransitionKind, DurableExternalEffectRequest, DurableInternalCommandRequest,
    LibraryFeedTransition, NonEmptyActivityFacts, RequestDisposition, RequestRejectionReason,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EpisodeStarredState {
    pub episode_id: EpisodeId,
    pub starred: bool,
    pub revision: StateRevision,
    pub legacy_command_revision: Option<StateRevision>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpisodeStarredMutation {
    Set {
        episode_id: EpisodeId,
        starred: bool,
    },
    RecordNoChange,
    LegacyDuplicate {
        committed_revision: StateRevision,
    },
}

pub type EpisodeStarredPlan = TransitionPlan<
    EpisodeStarredMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_episode_starred(
    command_id: CommandId,
    state: EpisodeStarredState,
    starred: bool,
) -> Result<EpisodeStarredPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(command_id);
    let transaction_id = identity.transaction_id();
    let disposition = if state.legacy_command_revision.is_some() {
        RequestDisposition::Duplicate
    } else if state.starred == starred {
        RequestDisposition::NoSemanticChange
    } else if state.revision.value.checked_add(1).is_none() {
        RequestDisposition::Rejected {
            reason: RequestRejectionReason::Invalid,
        }
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
        subject: ActivitySubject::Episode {
            episode_id: state.episode_id,
        },
        episode_id: Some(state.episode_id),
        fact,
    };
    let disposition_fact = base(0, ActivityFact::RequestDisposition { disposition });
    let (mutation, facts) = match disposition {
        RequestDisposition::Accepted => {
            let committed_revision = StateRevision::new(
                state
                    .revision
                    .value
                    .checked_add(1)
                    .expect("validated revision"),
            );
            (
                EpisodeStarredMutation::Set {
                    episode_id: state.episode_id,
                    starred,
                },
                NonEmptyActivityFacts::from_head_and_tail(
                    disposition_fact,
                    vec![base(
                        1,
                        ActivityFact::DomainTransition {
                            kind: DomainTransitionKind::LibraryFeed(
                                LibraryFeedTransition::EpisodeStarredChanged,
                            ),
                            previous_revision: state.revision,
                            committed_revision,
                        },
                    )],
                ),
            )
        }
        RequestDisposition::Duplicate => (
            EpisodeStarredMutation::LegacyDuplicate {
                committed_revision: state
                    .legacy_command_revision
                    .expect("duplicate has legacy revision"),
            },
            NonEmptyActivityFacts::new(disposition_fact),
        ),
        _ => (
            EpisodeStarredMutation::RecordNoChange,
            NonEmptyActivityFacts::new(disposition_fact),
        ),
    };
    TransitionPlan::new(
        transaction_id,
        state.revision,
        mutation,
        facts,
        Vec::new(),
        Vec::new(),
    )
}
