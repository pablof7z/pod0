use pod0_domain::{
    AgentExecutionFenceId, AgentTurnId, CancellationId, ConversationId, UnixTimestampMilliseconds,
};

use crate::*;

#[test]
fn older_persisted_turns_and_action_results_default_typed_recall_evidence() {
    let state = AgentTurnState::start(AgentTurnStart {
        conversation_id: ConversationId::from_parts(1, 1),
        turn_id: AgentTurnId::from_parts(1, 2),
        model_fence_id: AgentExecutionFenceId::from_parts(1, 3),
        user_input: "What did the episode say?".into(),
        model_reference: "openrouter/test".into(),
        available_tools: vec![AgentToolName::QueryTranscripts],
        cancellation_id: CancellationId::from_parts(1, 4),
        observed_at: UnixTimestampMilliseconds::new(10),
    })
    .unwrap();
    let mut state_json = serde_json::to_value(&state).unwrap();
    state_json["projection"]
        .as_object_mut()
        .unwrap()
        .remove("recall_evidence");
    let restored: AgentTurnState = serde_json::from_value(state_json).unwrap();
    assert!(restored.projection().recall_evidence.is_empty());

    let outcome = AgentActionOutcome::Succeeded {
        bounded_result: "saved".into(),
        artifact_id: None,
        recall_evidence: Vec::new(),
    };
    let mut outcome_json = serde_json::to_value(outcome).unwrap();
    outcome_json
        .as_object_mut()
        .unwrap()
        .get_mut("Succeeded")
        .and_then(serde_json::Value::as_object_mut)
        .unwrap()
        .remove("recall_evidence");
    let restored: AgentActionOutcome = serde_json::from_value(outcome_json).unwrap();
    assert!(matches!(
        restored,
        AgentActionOutcome::Succeeded {
            recall_evidence,
            ..
        } if recall_evidence.is_empty()
    ));
}
