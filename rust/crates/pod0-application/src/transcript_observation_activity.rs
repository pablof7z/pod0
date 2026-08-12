use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EffectAttemptId, EffectIntentId, EpisodeId,
    HostRequestId, StateRevision, TranscriptWorkflowId,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityFailureCode, ActivityOrigin,
    ActivitySubject, DomainTransitionKind, DurableExternalEffectRequest,
    DurableInternalCommandRequest, EffectObservationActivityIdentity, EffectOutcome,
    NonEmptyActivityFacts, RequestDisposition, TranscriptCapabilityObservation,
    TranscriptFailureEvidence, TranscriptTransition, TransitionPlan, TransitionPlanError,
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
    observation: &TranscriptCapabilityObservation,
) -> (EffectOutcome, TranscriptTransition) {
    match observation {
        TranscriptCapabilityObservation::ProviderAccepted { .. }
        | TranscriptCapabilityObservation::ProviderPending { .. } => (
            EffectOutcome::Succeeded,
            TranscriptTransition::AttemptStateChanged,
        ),
        TranscriptCapabilityObservation::Completed { .. } => (
            EffectOutcome::Succeeded,
            TranscriptTransition::ArtifactAdopted,
        ),
        TranscriptCapabilityObservation::Cancelled => {
            (EffectOutcome::Cancelled, TranscriptTransition::Cancelled)
        }
        TranscriptCapabilityObservation::Failed { evidence, .. } => (
            EffectOutcome::Failed {
                code: transcript_failure_activity_code(*evidence),
            },
            TranscriptTransition::AttemptStateChanged,
        ),
    }
}

const fn transcript_failure_activity_code(
    evidence: TranscriptFailureEvidence,
) -> ActivityFailureCode {
    match evidence {
        TranscriptFailureEvidence::Offline { .. } => ActivityFailureCode::Offline,
        TranscriptFailureEvidence::TimedOut { .. } => ActivityFailureCode::TimedOut,
        TranscriptFailureEvidence::PermissionDenied => ActivityFailureCode::PermissionDenied,
        TranscriptFailureEvidence::InvalidResponse
        | TranscriptFailureEvidence::InvalidRequest
        | TranscriptFailureEvidence::ProviderRejected
        | TranscriptFailureEvidence::StaleInput => ActivityFailureCode::InvalidResponse,
        TranscriptFailureEvidence::ResponseTooLarge => ActivityFailureCode::ResponseTooLarge,
        TranscriptFailureEvidence::MissingLocalAudio => ActivityFailureCode::MediaUnavailable,
        TranscriptFailureEvidence::MissingCredential => ActivityFailureCode::Unauthorized,
        TranscriptFailureEvidence::ProviderUnavailable { .. }
        | TranscriptFailureEvidence::ProviderRecoveryUnavailable
        | TranscriptFailureEvidence::PublisherUnavailable
        | TranscriptFailureEvidence::RateLimited { .. }
        | TranscriptFailureEvidence::RetryExhausted { .. }
        | TranscriptFailureEvidence::UnsupportedProvider => {
            ActivityFailureCode::ProviderUnavailable
        }
        TranscriptFailureEvidence::Transport { .. }
        | TranscriptFailureEvidence::StorageUnavailable { .. }
        | TranscriptFailureEvidence::Cancelled { .. }
        | TranscriptFailureEvidence::Unsupported { .. } => ActivityFailureCode::PlatformFailure,
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
