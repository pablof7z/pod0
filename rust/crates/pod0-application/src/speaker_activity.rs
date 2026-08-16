use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DomainTransitionKind, DurableExternalEffectRequest, DurableInternalCommandRequest,
    NonEmptyActivityFacts, RequestDisposition, TransitionPlan, TransitionPlanError,
    UserArtifactTransition,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpeakerActivityInput {
    pub command_id: CommandId,
    pub actor: ActivityActor,
    pub origin: ActivityOrigin,
    pub subject: ActivitySubject,
    pub episode_id: Option<EpisodeId>,
    pub current_revision: StateRevision,
    pub committed_revision: StateRevision,
    pub transition: UserArtifactTransition,
    pub disposition: RequestDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeakerMutation {
    Apply,
    None,
}

pub type SpeakerActivityPlan =
    TransitionPlan<SpeakerMutation, DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_speaker_activity(
    input: SpeakerActivityInput,
) -> Result<SpeakerActivityPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: input.actor,
        origin: input.origin,
        subject: input.subject,
        episode_id: input.episode_id,
        fact,
    };
    let accepted = input.disposition == RequestDisposition::Accepted;
    if accepted && input.committed_revision.value <= input.current_revision.value {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let disposition = base(
        0,
        ActivityFact::RequestDisposition {
            disposition: input.disposition,
        },
    );
    let facts = if accepted {
        NonEmptyActivityFacts::from_head_and_tail(
            disposition,
            vec![base(
                1,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::UserArtifact(input.transition),
                    previous_revision: input.current_revision,
                    committed_revision: input.committed_revision,
                },
            )],
        )
    } else {
        NonEmptyActivityFacts::new(disposition)
    };
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        if accepted {
            SpeakerMutation::Apply
        } else {
            SpeakerMutation::None
        },
        facts,
        Vec::new(),
        Vec::new(),
    )
}
