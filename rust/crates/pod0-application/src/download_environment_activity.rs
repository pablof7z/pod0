use pod0_domain::{CommandId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DomainTransitionKind, DownloadTransition, DurableExternalEffectRequest,
    DurableInternalCommandRequest, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DownloadEnvironmentActivityInput {
    pub command_id: CommandId,
    pub current_revision: StateRevision,
    pub legacy_replay: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadEnvironmentMutation {
    Apply,
    LegacyDuplicate,
}

pub type DownloadEnvironmentPlan = TransitionPlan<
    DownloadEnvironmentMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_download_environment(
    input: DownloadEnvironmentActivityInput,
) -> Result<DownloadEnvironmentPlan, TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let (mutation, disposition) = if input.legacy_replay {
        (
            DownloadEnvironmentMutation::LegacyDuplicate,
            RequestDisposition::Duplicate,
        )
    } else {
        (
            DownloadEnvironmentMutation::Apply,
            RequestDisposition::Accepted,
        )
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::System,
        origin: ActivityOrigin::HostObservation,
        subject: ActivitySubject::Global,
        episode_id: None,
        fact,
    };
    let facts = if mutation == DownloadEnvironmentMutation::Apply {
        let committed = StateRevision::new(
            input
                .current_revision
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        NonEmptyActivityFacts::from_head_and_tail(
            base(0, ActivityFact::RequestDisposition { disposition }),
            vec![base(
                1,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::Download(DownloadTransition::EnvironmentChanged),
                    previous_revision: input.current_revision,
                    committed_revision: committed,
                },
            )],
        )
    } else {
        NonEmptyActivityFacts::new(base(0, ActivityFact::RequestDisposition { disposition }))
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
