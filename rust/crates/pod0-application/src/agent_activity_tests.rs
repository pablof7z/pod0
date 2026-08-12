use pod0_domain::{
    ActivityCorrelationId, ActivityId, AgentTurnId, CommandId, EffectAttemptId, EffectIntentId,
    HostRequestId, StateRevision,
};

use crate::{
    AgentCancellationActivityInput, AgentCancellationMutation,
    AgentEffectObservationActivityInput, AgentPublicationTransition, AgentTurnStartActivityInput,
    AgentTurnStartMutation, EffectOutcome, ExternalEffectKind, RequestDisposition,
    RequestRejectionReason, plan_agent_cancellation, plan_agent_effect_observation,
    plan_agent_turn_start,
};

#[test]
fn agent_start_couples_state_transition_and_model_effect() {
    let plan = plan_agent_turn_start(AgentTurnStartActivityInput {
        command_id: CommandId::from_parts(1, 2),
        turn_id: AgentTurnId::from_parts(3, 4),
        current_revision: StateRevision::INITIAL,
        committed_revision: StateRevision::new(1),
        legacy_replay: false,
    })
    .unwrap();
    let (_, _, mutation, facts, effects, commands, disposition) = plan.into_parts();
    assert_eq!(mutation, AgentTurnStartMutation::Start);
    assert_eq!(disposition, RequestDisposition::Accepted);
    assert_eq!(facts.len(), 3);
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].request.kind, ExternalEffectKind::AgentProvider);
    assert!(commands.is_empty());
}

#[test]
fn cancellation_has_a_transition_only_when_accepted() {
    let accepted = plan_agent_cancellation(AgentCancellationActivityInput {
        command_id: CommandId::from_parts(8, 1),
        turn_id: AgentTurnId::from_parts(8, 2),
        current_revision: StateRevision::new(3),
        committed_revision: StateRevision::new(4),
        disposition: RequestDisposition::Accepted,
    })
    .unwrap();
    let (_, _, mutation, facts, effects, commands, _) = accepted.into_parts();
    assert_eq!(mutation, AgentCancellationMutation::Cancel);
    assert_eq!(facts.len(), 2);
    assert!(effects.is_empty());
    assert!(commands.is_empty());

    let rejected = plan_agent_cancellation(AgentCancellationActivityInput {
        command_id: CommandId::from_parts(8, 3),
        turn_id: AgentTurnId::from_parts(8, 2),
        current_revision: StateRevision::new(3),
        committed_revision: StateRevision::new(3),
        disposition: RequestDisposition::Rejected {
            reason: RequestRejectionReason::RevisionConflict,
        },
    })
    .unwrap();
    let (_, _, mutation, facts, effects, commands, _) = rejected.into_parts();
    assert_eq!(mutation, AgentCancellationMutation::None);
    assert_eq!(facts.len(), 1);
    assert!(effects.is_empty());
    assert!(commands.is_empty());
}

#[test]
fn model_observation_retires_exact_attempt_and_can_authorize_one_next_phase() {
    let cause = ActivityId::from_parts(6, 7);
    let plan = plan_agent_effect_observation(AgentEffectObservationActivityInput {
        command_id: CommandId::from_parts(1, 2),
        request_id: HostRequestId::from_parts(2, 3),
        turn_id: AgentTurnId::from_parts(3, 4),
        current_revision: StateRevision::new(1),
        committed_revision: StateRevision::new(2),
        intent_id: EffectIntentId::from_parts(4, 5),
        attempt_id: EffectAttemptId::from_parts(5, 6),
        authorizing_activity_id: cause,
        correlation_id: ActivityCorrelationId::from_parts(7, 8),
        episode_id: None,
        outcome: EffectOutcome::Succeeded,
        transition: AgentPublicationTransition::TurnStateChanged,
        next_effect: Some(ExternalEffectKind::AgentApproval),
        advance_turn: false,
    })
    .unwrap();
    let (_, _, _, facts, effects, commands, disposition) = plan.into_parts();
    assert_eq!(disposition, RequestDisposition::Accepted);
    assert_eq!(facts.len(), 4);
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].request.kind, ExternalEffectKind::AgentApproval);
    assert!(commands.is_empty());
    assert!(
        facts
            .iter()
            .all(|fact| fact.caused_by_activity_id == Some(cause))
    );
}

#[test]
fn legacy_agent_start_replay_never_authorizes_another_paid_call() {
    let plan = plan_agent_turn_start(AgentTurnStartActivityInput {
        command_id: CommandId::from_parts(1, 2),
        turn_id: AgentTurnId::from_parts(3, 4),
        current_revision: StateRevision::new(1),
        committed_revision: StateRevision::new(1),
        legacy_replay: true,
    })
    .unwrap();
    let (_, _, mutation, facts, effects, commands, disposition) = plan.into_parts();
    assert_eq!(mutation, AgentTurnStartMutation::LegacyDuplicate);
    assert_eq!(disposition, RequestDisposition::Duplicate);
    assert_eq!(facts.len(), 1);
    assert!(effects.is_empty());
    assert!(commands.is_empty());
}
