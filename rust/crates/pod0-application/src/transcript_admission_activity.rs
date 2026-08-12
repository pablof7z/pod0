use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EpisodeId, InternalCommandId, StateRevision,
    TranscriptWorkflowId,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    CommandActivityIdentity, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, NonEmptyActivityFacts, RequestDisposition, TranscriptTransition,
    TranscriptWorkflowOrigin, TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptAdmissionActivityInput {
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub workflow_id: TranscriptWorkflowId,
    pub current_workflow_revision: Option<StateRevision>,
    pub exact_replay: bool,
    pub origin: TranscriptWorkflowOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptInternalAdmissionActivityInput {
    pub internal_command_id: InternalCommandId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub episode_id: EpisodeId,
    pub workflow_id: TranscriptWorkflowId,
    pub current_workflow_revision: Option<StateRevision>,
    pub exact_replay: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptInternalDispositionActivityInput {
    pub internal_command_id: InternalCommandId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub episode_id: EpisodeId,
    pub state_revision: StateRevision,
    pub disposition: RequestDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptAdmissionMutation {
    Ensure,
    RecordDuplicate,
}

pub type TranscriptAdmissionPlan = TransitionPlan<
    TranscriptAdmissionMutation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptDispositionActivityInput {
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub state_revision: StateRevision,
    pub origin: TranscriptWorkflowOrigin,
    pub disposition: RequestDisposition,
}

pub fn plan_transcript_admission(
    input: TranscriptAdmissionActivityInput,
) -> Result<TranscriptAdmissionPlan, TransitionPlanError> {
    let identity = CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::TranscriptWorkflow {
        workflow_id: input.workflow_id,
    };
    let (actor, origin) = activity_origin(input.origin);
    let disposition = if input.exact_replay {
        RequestDisposition::Duplicate
    } else {
        RequestDisposition::Accepted
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor,
        origin,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    let current = input
        .current_workflow_revision
        .unwrap_or(StateRevision::INITIAL);
    let (mutation, facts) = if input.exact_replay {
        (
            TranscriptAdmissionMutation::RecordDuplicate,
            NonEmptyActivityFacts::new(base(0, ActivityFact::RequestDisposition { disposition })),
        )
    } else {
        let committed = StateRevision::new(
            current
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        (
            TranscriptAdmissionMutation::Ensure,
            NonEmptyActivityFacts::from_head_and_tail(
                base(0, ActivityFact::RequestDisposition { disposition }),
                vec![base(
                    1,
                    ActivityFact::DomainTransition {
                        kind: DomainTransitionKind::Transcript(
                            TranscriptTransition::WorkflowStateChanged,
                        ),
                        previous_revision: current,
                        committed_revision: committed,
                    },
                )],
            ),
        )
    };
    TransitionPlan::new(
        transaction_id,
        current,
        mutation,
        facts,
        Vec::new(),
        Vec::new(),
    )
}

pub fn plan_transcript_internal_admission(
    input: TranscriptInternalAdmissionActivityInput,
) -> Result<TranscriptAdmissionPlan, TransitionPlanError> {
    let identity = crate::InternalCommandActivityIdentity::new(input.internal_command_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::TranscriptWorkflow {
        workflow_id: input.workflow_id,
    };
    let disposition = if input.exact_replay {
        RequestDisposition::Duplicate
    } else {
        RequestDisposition::Accepted
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: None,
        host_request_id: None,
        actor: ActivityActor::System,
        origin: ActivityOrigin::InternalCommand,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    let current = input
        .current_workflow_revision
        .unwrap_or(StateRevision::INITIAL);
    let (mutation, facts) = if input.exact_replay {
        (
            TranscriptAdmissionMutation::RecordDuplicate,
            NonEmptyActivityFacts::new(base(0, ActivityFact::RequestDisposition { disposition })),
        )
    } else {
        let committed = StateRevision::new(
            current
                .value
                .checked_add(1)
                .ok_or(TransitionPlanError::RevisionExhausted)?,
        );
        (
            TranscriptAdmissionMutation::Ensure,
            NonEmptyActivityFacts::from_head_and_tail(
                base(0, ActivityFact::RequestDisposition { disposition }),
                vec![base(
                    1,
                    ActivityFact::DomainTransition {
                        kind: DomainTransitionKind::Transcript(
                            TranscriptTransition::WorkflowStateChanged,
                        ),
                        previous_revision: current,
                        committed_revision: committed,
                    },
                )],
            ),
        )
    };
    TransitionPlan::new(
        transaction_id,
        current,
        mutation,
        facts,
        Vec::new(),
        Vec::new(),
    )
}

pub fn plan_transcript_internal_disposition(
    input: TranscriptInternalDispositionActivityInput,
) -> Result<
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>,
    TransitionPlanError,
> {
    if input.disposition == RequestDisposition::Accepted {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let identity = crate::InternalCommandActivityIdentity::new(input.internal_command_id);
    let transaction_id = identity.transaction_id();
    let fact = ActivityFactDraft {
        activity_id: identity.fact_id(0),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: None,
        host_request_id: None,
        actor: ActivityActor::System,
        origin: ActivityOrigin::InternalCommand,
        subject: ActivitySubject::Episode {
            episode_id: input.episode_id,
        },
        episode_id: Some(input.episode_id),
        fact: ActivityFact::RequestDisposition {
            disposition: input.disposition,
        },
    };
    TransitionPlan::new(
        transaction_id,
        input.state_revision,
        (),
        NonEmptyActivityFacts::new(fact),
        Vec::new(),
        Vec::new(),
    )
}

pub fn plan_transcript_request_disposition(
    input: TranscriptDispositionActivityInput,
) -> Result<
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>,
    TransitionPlanError,
> {
    if input.disposition == RequestDisposition::Accepted {
        return Err(TransitionPlanError::DispositionRequiresTransition);
    }
    let identity = CommandActivityIdentity::new(input.command_id);
    let transaction_id = identity.transaction_id();
    let (actor, origin) = activity_origin(input.origin);
    let fact = ActivityFactDraft {
        activity_id: identity.fact_id(0),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: None,
        actor,
        origin,
        subject: ActivitySubject::Episode {
            episode_id: input.episode_id,
        },
        episode_id: Some(input.episode_id),
        fact: ActivityFact::RequestDisposition {
            disposition: input.disposition,
        },
    };
    TransitionPlan::new(
        transaction_id,
        input.state_revision,
        (),
        NonEmptyActivityFacts::new(fact),
        Vec::new(),
        Vec::new(),
    )
}

const fn activity_origin(origin: TranscriptWorkflowOrigin) -> (ActivityActor, ActivityOrigin) {
    match origin {
        TranscriptWorkflowOrigin::User => (ActivityActor::User, ActivityOrigin::UserInterface),
        TranscriptWorkflowOrigin::Automatic => {
            (ActivityActor::System, ActivityOrigin::AutomaticPolicy)
        }
        TranscriptWorkflowOrigin::Playback => (ActivityActor::System, ActivityOrigin::Playback),
        TranscriptWorkflowOrigin::Unsupported { .. } => {
            (ActivityActor::System, ActivityOrigin::AutomaticPolicy)
        }
    }
}
