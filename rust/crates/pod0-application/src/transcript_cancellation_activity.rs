use pod0_domain::{CommandId, EpisodeId, StateRevision, TranscriptWorkflowId};

use crate::{
    ActivitySubject, CancellationEffectTarget, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, TranscriptTransition, TransitionPlan, TransitionPlanError,
    WorkflowCancellationActivityInput, plan_workflow_cancellation_activity,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptCancellationActivityInput {
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub workflow_id: TranscriptWorkflowId,
    pub workflow_revision: StateRevision,
    pub target: Option<CancellationEffectTarget>,
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
    let subject = ActivitySubject::TranscriptWorkflow {
        workflow_id: input.workflow_id,
    };
    plan_workflow_cancellation_activity(WorkflowCancellationActivityInput {
        command_id: input.command_id,
        episode_id: input.episode_id,
        subject,
        current_revision: input.workflow_revision,
        transition: DomainTransitionKind::Transcript(TranscriptTransition::Cancelled),
        target: input.target,
    })
    .map(|plan| plan.map_mutation(|()| CancelTranscriptWorkflow))
}
