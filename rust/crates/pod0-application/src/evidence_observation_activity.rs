use pod0_domain::{
    ActivityCorrelationId, ActivityId, EffectAttemptId, EffectIntentId, EpisodeId,
    EvidenceGenerationId, HostRequestId, StateRevision,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    DomainTransitionKind, DurableExternalEffectRequest, DurableInternalCommandRequest,
    DurableRecallHostObservation, EffectObservationActivityIdentity, EffectOutcome,
    NonEmptyActivityFacts, RecallKnowledgeTransition, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceObservationActivityInput {
    pub request_id: HostRequestId,
    pub episode_id: EpisodeId,
    pub generation_id: EvidenceGenerationId,
    pub intent_id: EffectIntentId,
    pub attempt_id: EffectAttemptId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub current_revision: StateRevision,
    pub observation: DurableRecallHostObservation,
}

pub type EvidenceObservationPlan = TransitionPlan<
    DurableRecallHostObservation,
    DurableExternalEffectRequest,
    DurableInternalCommandRequest,
>;

pub fn plan_evidence_observation(
    input: EvidenceObservationActivityInput,
) -> Result<EvidenceObservationPlan, TransitionPlanError> {
    let identity = EffectObservationActivityIdentity::new(input.attempt_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::Episode {
        episode_id: input.episode_id,
    };
    let base = |ordinal, fact| ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.authorizing_activity_id),
        command_id: None,
        host_request_id: Some(input.request_id),
        actor: ActivityActor::System,
        origin: ActivityOrigin::HostObservation,
        subject,
        episode_id: Some(input.episode_id),
        fact,
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
        input.observation,
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
                        outcome: EffectOutcome::Succeeded,
                    },
                ),
                base(
                    2,
                    ActivityFact::DomainTransition {
                        kind: DomainTransitionKind::RecallKnowledge(
                            RecallKnowledgeTransition::EvidenceGenerationChanged,
                        ),
                        previous_revision: input.current_revision,
                        committed_revision,
                    },
                ),
            ],
        ),
        Vec::new(),
        Vec::new(),
    )
}
