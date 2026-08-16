use pod0_domain::{CommandId, EpisodeId, HostRequestId, StateRevision, TranscriptWorkflowId};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DomainTransitionKind, DurableExternalEffectRequest, DurableInternalCommandRequest,
    NonEmptyActivityFacts, RequestDisposition, TranscriptTransition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptAmbiguousRecoveryInput {
    pub recovery_id: CommandId,
    pub command_id: CommandId,
    pub request_id: HostRequestId,
    pub episode_id: EpisodeId,
    pub workflow_id: TranscriptWorkflowId,
    pub current_revision: StateRevision,
}

pub fn plan_transcript_ambiguous_recovery(
    input: TranscriptAmbiguousRecoveryInput,
) -> Result<
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>,
    TransitionPlanError,
> {
    let identity = crate::CommandActivityIdentity::new(input.recovery_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::TranscriptWorkflow {
        workflow_id: input.workflow_id,
    };
    let fact = |ordinal, value| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: Some(input.request_id),
        actor: ActivityActor::Recovery,
        origin: ActivityOrigin::Recovery,
        subject,
        episode_id: Some(input.episode_id),
        fact: value,
    };
    let committed_revision = StateRevision::new(
        input
            .current_revision
            .value
            .checked_add(1)
            .ok_or(TransitionPlanError::RevisionExhausted)?,
    );
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
        NonEmptyActivityFacts::from_head_and_tail(
            fact(
                0,
                ActivityFact::RequestDisposition {
                    disposition: RequestDisposition::Accepted,
                },
            ),
            vec![fact(
                1,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::Transcript(
                        TranscriptTransition::AttemptStateChanged,
                    ),
                    previous_revision: input.current_revision,
                    committed_revision,
                },
            )],
        ),
        Vec::new(),
        Vec::new(),
    )
}
