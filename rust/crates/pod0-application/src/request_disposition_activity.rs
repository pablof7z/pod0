use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DurableExternalEffectRequest, DurableInternalCommandRequest, NonEmptyActivityFacts,
    RequestDisposition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestDispositionActivityInput {
    pub command_id: CommandId,
    pub subject: ActivitySubject,
    pub episode_id: Option<EpisodeId>,
    pub current_revision: StateRevision,
    pub actor: ActivityActor,
    pub origin: ActivityOrigin,
    pub disposition: RequestDisposition,
}

pub type RequestDispositionPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_request_disposition(
    input: RequestDispositionActivityInput,
) -> Result<RequestDispositionPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        NonEmptyActivityFacts::new(ActivityFactDraft {
            activity_id: identity.fact_id(0),
            transaction_id,
            correlation_id: identity.correlation_id(),
            caused_by_activity_id: None,
            command_id: Some(input.command_id),
            host_request_id: None,
            actor: input.actor,
            origin: input.origin,
            subject: input.subject,
            episode_id: input.episode_id,
            fact: ActivityFact::RequestDisposition {
                disposition: input.disposition,
            },
        }),
        Vec::new(),
        Vec::new(),
    )
}
