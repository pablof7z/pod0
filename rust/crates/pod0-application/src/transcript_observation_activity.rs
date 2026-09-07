use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EffectAttemptId, EffectIntentId, EpisodeId,
    HostRequestId, StateRevision, TranscriptWorkflowId,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityFailureCode, ActivityOrigin,
    ActivitySubject, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, EffectObservationActivityIdentity, EffectOutcome,
    NonEmptyActivityFacts, RequestDisposition, TranscriptFailureTransition,
    TranscriptObservationDecision, TranscriptTransition, TranscriptWorkflowFailureCode,
    TransitionPlan, TransitionPlanError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptObservationActivityInput {
    pub command_id: CommandId,
    pub request_id: HostRequestId,
    pub episode_id: EpisodeId,
    pub workflow_id: TranscriptWorkflowId,
    pub workflow_revision: StateRevision,
    pub intent_id: EffectIntentId,
    pub attempt_id: EffectAttemptId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub outcome: EffectOutcome,
    pub transition: TranscriptTransition,
}

#[must_use]
pub fn transcript_observation_semantics(
    decision: &TranscriptObservationDecision,
) -> (EffectOutcome, TranscriptTransition) {
    match decision {
        TranscriptObservationDecision::ProviderAccepted { .. }
        | TranscriptObservationDecision::ProviderPending { .. } => (
            EffectOutcome::Progressed,
            TranscriptTransition::AttemptStateChanged,
        ),
        TranscriptObservationDecision::Completion => (
            EffectOutcome::Succeeded,
            TranscriptTransition::ArtifactAdopted,
        ),
        TranscriptObservationDecision::Failure {
            transition: TranscriptFailureTransition::Cancel,
            ..
        } => (EffectOutcome::Cancelled, TranscriptTransition::Cancelled),
        TranscriptObservationDecision::Failure {
            transition: TranscriptFailureTransition::Ambiguous,
            ..
        } => (
            EffectOutcome::OutcomeUnknown,
            TranscriptTransition::AttemptStateChanged,
        ),
        TranscriptObservationDecision::Failure { code, .. } => (
            EffectOutcome::Failed {
                code: transcript_failure_activity_code(*code),
            },
            TranscriptTransition::AttemptStateChanged,
        ),
    }
}

const fn transcript_failure_activity_code(
    code: TranscriptWorkflowFailureCode,
) -> ActivityFailureCode {
    match code {
        TranscriptWorkflowFailureCode::Offline => ActivityFailureCode::Offline,
        TranscriptWorkflowFailureCode::TimedOut => ActivityFailureCode::TimedOut,
        TranscriptWorkflowFailureCode::PermissionDenied => ActivityFailureCode::PermissionDenied,
        TranscriptWorkflowFailureCode::InvalidResponse
        | TranscriptWorkflowFailureCode::InvalidRequest
        | TranscriptWorkflowFailureCode::ProviderRejected
        | TranscriptWorkflowFailureCode::StaleInput => ActivityFailureCode::InvalidResponse,
        TranscriptWorkflowFailureCode::ResponseTooLarge => ActivityFailureCode::ResponseTooLarge,
        TranscriptWorkflowFailureCode::MissingLocalAudio => ActivityFailureCode::MediaUnavailable,
        TranscriptWorkflowFailureCode::MissingCredential => ActivityFailureCode::Unauthorized,
        TranscriptWorkflowFailureCode::ProviderUnavailable
        | TranscriptWorkflowFailureCode::ProviderRecoveryUnavailable
        | TranscriptWorkflowFailureCode::PublisherUnavailable
        | TranscriptWorkflowFailureCode::RateLimited
        | TranscriptWorkflowFailureCode::RetryExhausted
        | TranscriptWorkflowFailureCode::UnsupportedProvider => {
            ActivityFailureCode::ProviderUnavailable
        }
        TranscriptWorkflowFailureCode::StorageUnavailable => {
            ActivityFailureCode::StorageUnavailable
        }
        TranscriptWorkflowFailureCode::Transport
        | TranscriptWorkflowFailureCode::AmbiguousSubmission
        | TranscriptWorkflowFailureCode::Cancelled => ActivityFailureCode::PlatformFailure,
        TranscriptWorkflowFailureCode::Unsupported { wire_code } => {
            ActivityFailureCode::Unsupported { wire_code }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplyTranscriptObservation;

pub type TranscriptObservationPlan = TransitionPlan<
    ApplyTranscriptObservation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_transcript_observation(
    input: TranscriptObservationActivityInput,
) -> Result<TranscriptObservationPlan, TransitionPlanError> {
    let identity = EffectObservationActivityIdentity::new(input.attempt_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::TranscriptWorkflow {
        workflow_id: input.workflow_id,
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: Some(input.command_id),
        host_request_id: Some(input.request_id),
        actor: ActivityActor::System,
        origin: ActivityOrigin::HostObservation,
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
        ApplyTranscriptObservation,
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
                    ActivityFact::EffectObserved {
                        intent_id: input.intent_id,
                        attempt_id: input.attempt_id,
                        outcome: input.outcome,
                    },
                ),
                base(
                    2,
                    ActivityFact::DomainTransition {
                        kind: DomainTransitionKind::Transcript(input.transition),
                        previous_revision: input.workflow_revision,
                        committed_revision,
                    },
                ),
            ],
        ),
        Vec::new(),
        Vec::new(),
    )
}
