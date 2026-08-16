use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EpisodeId, HostRequestId, InternalCommandId,
    StateRevision,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    ChapterRecordedTransition, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, InternalCommandActivityIdentity, NonEmptyActivityFacts,
    RequestDisposition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterFinalizationActivityInput {
    pub internal_command_id: InternalCommandId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub command_id: CommandId,
    pub request_id: HostRequestId,
    pub episode_id: EpisodeId,
    pub current_workflow_revision: StateRevision,
    pub transitions: Vec<ChapterRecordedTransition>,
}

pub type ChapterFinalizationActivityPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_chapter_finalization_activity(
    input: ChapterFinalizationActivityInput,
) -> Result<ChapterFinalizationActivityPlan, TransitionPlanError> {
    let identity = InternalCommandActivityIdentity::new(input.internal_command_id);
    let transaction_id = identity.transaction_id();
    let committed_workflow_revision = StateRevision::new(
        input
            .current_workflow_revision
            .value
            .checked_add(1)
            .ok_or(TransitionPlanError::RevisionExhausted)?,
    );
    let Some(first) = input.transitions.first() else {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    };
    if first.previous_revision != input.current_workflow_revision
        || first.committed_revision != committed_workflow_revision
    {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
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
        origin: ActivityOrigin::InternalCommand,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    let transitions = input
        .transitions
        .iter()
        .enumerate()
        .map(|(index, transition)| {
            base(
                u8::try_from(index + 1).expect("bounded chapter finalization facts"),
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::Chapter(transition.kind),
                    previous_revision: transition.previous_revision,
                    committed_revision: transition.committed_revision,
                },
            )
        })
        .collect();
    TransitionPlan::new(
        transaction_id,
        input.current_workflow_revision,
        (),
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: RequestDisposition::Accepted,
                },
            ),
            transitions,
        ),
        Vec::new(),
        Vec::new(),
    )
}
