use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EpisodeId, HostRequestId, InternalCommandId,
    StateRevision,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DomainTransitionKind, DownloadTransition, DurableExternalEffectRequest,
    DurableInternalCommandRequest, InternalCommandActivityIdentity, NonEmptyActivityFacts,
    RequestDisposition, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DownloadFinalizationActivityInput {
    pub internal_command_id: InternalCommandId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub command_id: CommandId,
    pub request_id: HostRequestId,
    pub episode_id: EpisodeId,
    pub current_revision: StateRevision,
}

pub type DownloadFinalizationPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_download_finalization(
    input: DownloadFinalizationActivityInput,
) -> Result<DownloadFinalizationPlan, TransitionPlanError> {
    let identity = InternalCommandActivityIdentity::new(input.internal_command_id);
    let transaction_id = identity.transaction_id();
    let committed_revision = StateRevision::new(
        input
            .current_revision
            .value
            .checked_add(1)
            .ok_or(TransitionPlanError::RevisionExhausted)?,
    );
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
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: RequestDisposition::Accepted,
                },
            ),
            vec![base(
                1,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::Download(DownloadTransition::AttemptStateChanged),
                    previous_revision: input.current_revision,
                    committed_revision,
                },
            )],
        ),
        Vec::new(),
        Vec::new(),
    )
}
