use pod0_domain::{
    ActivityCorrelationId, ActivityId, AgentTurnId, CommandId, EffectAttemptId, EffectIntentId,
    HostRequestId, StateRevision,
};

use crate::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
    AuthorizedExternalEffect, DurableEffectExecution, DurableExternalEffectRequest,
    DurableInternalCommandRequest, EffectObservationActivityIdentity, EffectOutcome,
    ExternalEffectKind, NonEmptyActivityFacts, RequestDisposition, TransitionPlan,
    TransitionPlanError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRecallProgressActivityInput {
    pub command_id: CommandId,
    pub request_id: HostRequestId,
    pub turn_id: AgentTurnId,
    pub current_revision: StateRevision,
    pub intent_id: EffectIntentId,
    pub attempt_id: EffectAttemptId,
    pub authorizing_activity_id: ActivityId,
    pub correlation_id: ActivityCorrelationId,
    pub next_request: crate::DurableAgentRecallEffectRequest,
}

pub type AgentRecallProgressPlan =
    TransitionPlan<(), DurableExternalEffectRequest, DurableInternalCommandRequest>;

pub fn plan_agent_recall_progress(
    input: AgentRecallProgressActivityInput,
) -> Result<AgentRecallProgressPlan, TransitionPlanError> {
    let identity = EffectObservationActivityIdentity::new(input.attempt_id);
    let transaction_id = identity.transaction_id();
    let subject = ActivitySubject::AgentTurn {
        turn_id: input.turn_id,
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
        episode_id: None,
        fact,
    };
    let next_intent = identity.effect_intent_id(0);
    TransitionPlan::new(
        transaction_id,
        input.current_revision,
        (),
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
                        outcome: EffectOutcome::Progressed,
                    },
                ),
                base(
                    2,
                    ActivityFact::EffectAuthorized {
                        intent_id: next_intent,
                        kind: ExternalEffectKind::RecallProvider,
                    },
                ),
            ],
        ),
        vec![AuthorizedExternalEffect {
            intent_id: next_intent,
            authorizing_fact_index: 2,
            request: DurableExternalEffectRequest {
                kind: ExternalEffectKind::RecallProvider,
                subject,
                episode_id: None,
                not_before: None,
                deadline_at: Some(input.next_request.deadline_at),
                execution: DurableEffectExecution::AgentRecall {
                    request: input.next_request,
                },
            },
        }],
        Vec::new(),
    )
}
