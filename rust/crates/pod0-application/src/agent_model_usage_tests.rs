use pod0_domain::{
    AgentExecutionFenceId, AgentTurnId, CancellationId, ConversationId, UnixTimestampMilliseconds,
};

use crate::*;

#[test]
fn valid_model_usage_becomes_durable_turn_evidence() {
    let mut state = AgentTurnState::start(AgentTurnStart {
        conversation_id: ConversationId::from_parts(1, 1),
        turn_id: AgentTurnId::from_parts(1, 2),
        model_fence_id: AgentExecutionFenceId::from_parts(1, 3),
        user_input: "What preserves options?".into(),
        model_reference: "openrouter/test".into(),
        available_tools: vec![AgentToolName::CreateNote],
        cancellation_id: CancellationId::from_parts(1, 4),
        observed_at: UnixTimestampMilliseconds::new(10),
    })
    .unwrap();

    assert_eq!(
        state.observe_model(AgentModelObservation {
            turn_id: AgentTurnId::from_parts(1, 2),
            model_fence_id: AgentExecutionFenceId::from_parts(1, 3),
            assistant_text: "Architecture preserves options.".into(),
            proposed_action: None,
            usage: Some(AgentModelUsageObservation {
                prompt_tokens: 120,
                completion_tokens: 24,
                cached_prompt_tokens: Some(40),
            }),
            observed_at: UnixTimestampMilliseconds::new(20),
        }),
        AgentWorkflowAcceptance::Updated
    );

    assert_eq!(state.projection().model_usage.len(), 1);
    let usage = &state.projection().model_usage[0];
    assert_eq!(usage.model_reference, "openrouter/test");
    assert_eq!(usage.prompt_tokens, 120);
    assert_eq!(usage.completion_tokens, 24);
    assert_eq!(usage.cached_prompt_tokens, Some(40));
}
