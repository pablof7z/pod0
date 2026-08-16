use pod0_domain::{
    CommandId, EpisodeId, HostRequestId, StateRevision, TranscriptWorkflowId,
    UnixTimestampMilliseconds,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, ExternalEffectKind, NonEmptyActivityFacts, RequestDisposition,
    TranscriptEffectActivityIdentity, TranscriptTransition, TranscriptWorkflowOrigin,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptSubmissionActivityInput {
    pub request_id: HostRequestId,
    pub command_id: CommandId,
    pub episode_id: EpisodeId,
    pub workflow_id: TranscriptWorkflowId,
    pub workflow_revision: StateRevision,
    pub origin: TranscriptWorkflowOrigin,
    pub deadline_at: Option<UnixTimestampMilliseconds>,
    pub execution: crate::DurableTranscriptEffectRequest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthorizeTranscriptSubmission;

pub type TranscriptSubmissionTransitionPlan = TransitionPlan<
    AuthorizeTranscriptSubmission,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_transcript_submission(
    input: TranscriptSubmissionActivityInput,
) -> Result<TranscriptSubmissionTransitionPlan, TransitionPlanError> {
    let identity = TranscriptEffectActivityIdentity::new(input.request_id, input.workflow_revision);
    let transaction_id = identity.transaction_id();
    let correlation_id = identity.correlation_id();
    let subject = ActivitySubject::TranscriptWorkflow {
        workflow_id: input.workflow_id,
    };
    let (actor, origin) = activity_origin(input.origin);
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id,
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: Some(input.request_id),
        actor,
        origin,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    let committed_revision = StateRevision::new(
        input
            .workflow_revision
            .value
            .checked_add(1)
            .expect("workflow revision validated before planning"),
    );
    let intent_id = identity.effect_intent_id(0);
    let request = DurableExternalEffectRequest {
        kind: ExternalEffectKind::TranscriptProvider,
        subject,
        episode_id: Some(input.episode_id),
        not_before: None,
        deadline_at: input.deadline_at,
        execution: crate::DurableEffectExecution::Transcript {
            request: input.execution.clone(),
        },
    };
    TransitionPlan::new(
        transaction_id,
        input.workflow_revision,
        AuthorizeTranscriptSubmission,
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
                            TranscriptTransition::AttemptStateChanged,
                        ),
                        previous_revision: input.workflow_revision,
                        committed_revision,
                    },
                ),
                base(
                    2,
                    ActivityFact::EffectAuthorized {
                        intent_id,
                        kind: ExternalEffectKind::TranscriptProvider,
                    },
                ),
            ],
        ),
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: 2,
            request,
        }],
        Vec::new(),
    )
}

pub fn plan_transcript_recovery_effect(
    input: TranscriptSubmissionActivityInput,
) -> Result<
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>,
    TransitionPlanError,
> {
    plan_transcript_stateless_effect(input, ActivityActor::Recovery, ActivityOrigin::Recovery)
}

pub fn plan_transcript_publisher_effect(
    input: TranscriptSubmissionActivityInput,
) -> Result<
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>,
    TransitionPlanError,
> {
    let (actor, origin) = activity_origin(input.origin);
    plan_transcript_stateless_effect(input, actor, origin)
}

fn plan_transcript_stateless_effect(
    input: TranscriptSubmissionActivityInput,
    actor: ActivityActor,
    origin: ActivityOrigin,
) -> Result<
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>,
    TransitionPlanError,
> {
    let identity = TranscriptEffectActivityIdentity::new(input.request_id, input.workflow_revision);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::TranscriptWorkflow {
        workflow_id: input.workflow_id,
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: identity.correlation_id(),
        caused_by_activity_id: None,
        command_id: Some(input.command_id),
        host_request_id: Some(input.request_id),
        actor,
        origin,
        subject,
        episode_id: Some(input.episode_id),
        fact,
    };
    let intent_id = identity.effect_intent_id(0);
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
                ActivityFact::EffectAuthorized {
                    intent_id,
                    kind: ExternalEffectKind::TranscriptProvider,
                },
            )],
        ),
        vec![AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: 1,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::TranscriptProvider,
                subject,
                episode_id: Some(input.episode_id),
                not_before: None,
                deadline_at: input.deadline_at,
                execution: crate::DurableEffectExecution::Transcript {
                    request: input.execution.clone(),
                },
            },
        }],
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
