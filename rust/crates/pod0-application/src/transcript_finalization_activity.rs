use pod0_domain::{CommandId, EpisodeId, StateRevision, TranscriptWorkflowId};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityFactDraft, ActivityOrigin,
    ActivitySubject, AuthorizedInternalCommand, CommandActivityIdentity, DomainTransitionKind,
    DurableExternalEffectRequest, DurableInternalCommandRequest, InternalCommandKind,
    NonEmptyActivityFacts, RequestDisposition, TranscriptTransition,
    TranscriptWorkflowActivityIdentity, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptFinalizationActivityInput {
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub workflow_id: TranscriptWorkflowId,
    pub workflow_revision: StateRevision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommitTranscriptAndRequestEvidence;

pub type TranscriptFinalizationPlan = TransitionPlan<
    CommitTranscriptAndRequestEvidence,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_transcript_finalization(
    input: TranscriptFinalizationActivityInput,
) -> Result<TranscriptFinalizationPlan, TransitionPlanError> {
    let identity = TranscriptWorkflowActivityIdentity::new(
        input.workflow_id,
        input.workflow_revision,
        TranscriptWorkflowActivityIdentity::FINALIZATION_PHASE,
    );
    let command_identity = CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let internal_command_id = identity.internal_command_id(0);
    let subject = ActivitySubject::TranscriptWorkflow {
        workflow_id: input.workflow_id,
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: command_identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::System,
        origin: ActivityOrigin::AutomaticPolicy,
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
        CommitTranscriptAndRequestEvidence,
        NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                ActivityFact::RequestDisposition {
                    disposition: RequestDisposition::Accepted,
                },
            ),
            vec![
                base(
                    1,
                    ActivityFact::DomainTransition {
                        kind: DomainTransitionKind::Transcript(
                            TranscriptTransition::SelectionChanged,
                        ),
                        previous_revision: input.workflow_revision,
                        committed_revision,
                    },
                ),
                base(
                    2,
                    ActivityFact::DomainTransition {
                        kind: DomainTransitionKind::Transcript(
                            TranscriptTransition::WorkflowStateChanged,
                        ),
                        previous_revision: input.workflow_revision,
                        committed_revision,
                    },
                ),
                base(
                    3,
                    ActivityFact::InternalCommandAuthorized {
                        internal_command_id,
                        target: ActivityDomain::RecallKnowledge,
                    },
                ),
            ],
        ),
        Vec::new(),
        vec![AuthorizedInternalCommand {
            internal_command_id,
            authorizing_fact_index: 3,
            command: DurableInternalCommandRequest {
                kind: InternalCommandKind::BuildTranscriptEvidence,
                target: ActivityDomain::RecallKnowledge,
                subject,
                episode_id: Some(input.episode_id),
            },
        }],
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptEvidenceCompletionActivityInput {
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub workflow_id: TranscriptWorkflowId,
    pub workflow_revision: StateRevision,
}

pub fn plan_transcript_evidence_completion(
    input: TranscriptEvidenceCompletionActivityInput,
) -> Result<
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>,
    TransitionPlanError,
> {
    let identity = TranscriptWorkflowActivityIdentity::new(
        input.workflow_id,
        input.workflow_revision,
        TranscriptWorkflowActivityIdentity::EVIDENCE_COMPLETION_PHASE,
    );
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::TranscriptWorkflow {
        workflow_id: input.workflow_id,
    };
    let committed_revision = StateRevision::new(
        input
            .workflow_revision
            .value
            .checked_add(1)
            .ok_or(TransitionPlanError::RevisionExhausted)?,
    );
    let command_identity = CommandActivityIdentity::new(input.command_id);
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: command_identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor: ActivityActor::System,
        origin: ActivityOrigin::AutomaticPolicy,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    TransitionPlan::new(
        transaction_id,
        input.workflow_revision,
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
                    kind: DomainTransitionKind::Transcript(
                        TranscriptTransition::WorkflowStateChanged,
                    ),
                    previous_revision: input.workflow_revision,
                    committed_revision,
                },
            )],
        ),
        Vec::new(),
        Vec::new(),
    )
}
