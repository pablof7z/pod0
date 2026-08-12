use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DomainTransitionKind, DurableExternalEffectRequest, DurableInternalCommandRequest,
    NonEmptyActivityFacts, RequestDisposition, TransitionPlan, TransitionPlanError,
    UserArtifactTransition,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserArtifactActivityInput {
    pub command_id: CommandId,
    pub subject: ActivitySubject,
    pub episode_ids: Vec<EpisodeId>,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub transition: UserArtifactTransition,
    pub disposition: RequestDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserArtifactMutation {
    Apply,
    None,
}

pub type UserArtifactActivityPlan = TransitionPlan<
    UserArtifactMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_user_artifact_activity(
    mut input: UserArtifactActivityInput,
) -> Result<UserArtifactActivityPlan, TransitionPlanError> {
    let mut distinct = Vec::with_capacity(input.episode_ids.len());
    for episode_id in input.episode_ids.drain(..) {
        if !distinct.contains(&episode_id) {
            distinct.push(episode_id);
        }
    }
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let base = |ordinal, episode_id, fact| ActivityFactDraft {
        activity_id: identity.fact_id_wide(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject: input.subject,
        episode_id,
        fact,
    };
    let accepted = input.disposition == RequestDisposition::Accepted;
    if accepted && input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let disposition = base(
        0,
        distinct.first().copied(),
        ActivityFact::RequestDisposition {
            disposition: input.disposition,
        },
    );
    let facts = if accepted {
        let episode_ids = if distinct.is_empty() {
            vec![None]
        } else {
            distinct.into_iter().map(Some).collect()
        };
        let transitions = episode_ids
            .into_iter()
            .enumerate()
            .map(|(index, episode_id)| {
                base(
                    u32::try_from(index + 1).expect("artifact fact count fits u32"),
                    episode_id,
                    ActivityFact::DomainTransition {
                        kind: DomainTransitionKind::UserArtifact(input.transition),
                        previous_revision: input.current_revision,
                        committed_revision: input.committed_revision,
                    },
                )
            })
            .collect();
        NonEmptyActivityFacts::from_head_and_tail(disposition, transitions)
    } else {
        NonEmptyActivityFacts::new(disposition)
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        if accepted {
            UserArtifactMutation::Apply
        } else {
            UserArtifactMutation::None
        },
        facts,
        Vec::new(),
        Vec::new(),
    )
}
