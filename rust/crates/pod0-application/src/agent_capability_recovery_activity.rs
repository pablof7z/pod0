#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentCapabilityRecoveryActivityInput {
    pub recovery_id: pod0_domain::CommandId,
    pub original_intent_id: pod0_domain::EffectIntentId,
    pub original_attempt_id: pod0_domain::EffectAttemptId,
    pub original_authorizing_activity_id: pod0_domain::ActivityId,
    pub correlation_id: pod0_domain::ActivityCorrelationId,
    pub turn_id: pod0_domain::AgentTurnId,
    pub current_revision: pod0_domain::StateRevision,
    pub committed_revision: pod0_domain::StateRevision,
    pub recovery: Option<crate::DurableAgentCapabilityEffectRequest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetireAmbiguousAgentCapability;

pub fn plan_agent_capability_recovery(
    input: AgentCapabilityRecoveryActivityInput,
) -> Result<AgentExecutionPlan, crate::TransitionPlanError> {
    let identity = crate::CommandActivityIdentity::new(input.recovery_id);
    let transaction_id = identity.transaction_id();
    let subject = crate::ActivitySubject::AgentTurn {
        turn_id: input.turn_id,
    };
    let base = |ordinal, fact| crate::ActivityFactDraft {
        activity_id: identity.fact_id(ordinal),
        transaction_id,
        correlation_id: input.correlation_id,
        caused_by_activity_id: Some(input.original_authorizing_activity_id),
        command_id: Some(input.recovery_id),
        host_request_id: None,
        actor: crate::ActivityActor::System,
        origin: crate::ActivityOrigin::Recovery,
        subject,
        episode_id: None,
        fact,
    };
    let mut tail = vec![
        base(
            1,
            crate::ActivityFact::EffectObserved {
                intent_id: input.original_intent_id,
                attempt_id: input.original_attempt_id,
                outcome: crate::EffectOutcome::OutcomeUnknown,
            },
        ),
        base(
            2,
            crate::ActivityFact::RecoveryTransition {
                outcome: crate::EffectOutcome::OutcomeUnknown,
            },
        ),
    ];
    let effects = if let Some(request) = input.recovery {
        let intent_id = identity.effect_intent_id(0);
        tail.push(base(
            3,
            crate::ActivityFact::EffectAuthorized {
                intent_id,
                kind: crate::ExternalEffectKind::AgentCapability,
            },
        ));
        vec![crate::AuthorizedExternalEffect {
            intent_id,
            authorizing_fact_index: 3,
            request: crate::DurableExternalEffectRequest {
                kind: crate::ExternalEffectKind::AgentCapability,
                subject,
                episode_id: None,
                not_before: None,
                deadline_at: request.deadline_at,
                execution: crate::DurableEffectExecution::AgentCapability { request },
            },
        }]
    } else {
        tail.push(base(
            3,
            crate::ActivityFact::DomainTransition {
                kind: crate::DomainTransitionKind::AgentPublication(
                    crate::AgentPublicationTransition::ToolStateChanged,
                ),
                previous_revision: input.current_revision,
                committed_revision: input.committed_revision,
            },
        ));
        Vec::new()
    };
    if effects.is_empty()
        && input.committed_revision.value != input.current_revision.value.saturating_add(1)
    {
        return Err(crate::TransitionPlanError::DispositionRequiresTransition);
    }
    if !effects.is_empty() && input.committed_revision != input.current_revision {
        return Err(crate::TransitionPlanError::DispositionRequiresTransition);
    }
    crate::TransitionPlan::new(
        transaction_id,
        input.current_revision,
        BeginAgentExecution,
        crate::NonEmptyActivityFacts::from_head_and_tail(
            base(
                0,
                crate::ActivityFact::RequestDisposition {
                    disposition: crate::RequestDisposition::Accepted,
                },
            ),
            tail,
        ),
        effects,
        Vec::new(),
    )
}
