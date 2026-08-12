use pod0_domain::{CommandId, EpisodeId, StateRevision, TranscriptWorkflowId};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    CommandActivityIdentity, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, NonEmptyActivityFacts, RequestDisposition, TranscriptTransition,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptCancellationActivityInput {
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub workflow_id: TranscriptWorkflowId,
    pub workflow_revision: StateRevision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CancelTranscriptWorkflow;

pub type TranscriptCancellationPlan = TransitionPlan<
    CancelTranscriptWorkflow,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_transcript_cancellation(
    input: TranscriptCancellationActivityInput,
) -> Result<TranscriptCancellationPlan, TransitionPlanError> {
    let identity = CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let correlation_id = identity.correlation_id();
    let subject = ActivitySubject::TranscriptWorkflow {
        workflow_id: input.workflow_id,
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id,
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::User,
        origin: ActivityOrigin::UserInterface,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    let committed_revision = StateRevision::new(
        input
            .workflow_revision
            .value
            .checked_add(1)
            .ok_or(TransitionPlanError::RevisionExhausted)?,
    );
    TransitionPlan::new(
        transaction_id,
        input.workflow_revision,
        CancelTranscriptWorkflow,
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
                    kind: DomainTransitionKind::Transcript(TranscriptTransition::Cancelled),
                    previous_revision: input.workflow_revision,
                    committed_revision,
                },
            )],
        ),
        Vec::new(),
        Vec::new(),
    )
}
