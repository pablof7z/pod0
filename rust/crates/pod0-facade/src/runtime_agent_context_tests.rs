use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn follow_up_turn_model_request_contains_bounded_conversation_context() {
    let fixture = PlaybackFixture::new();
    let first = start_command(4, None, "Save architecture matters as a note");
    fixture.facade.dispatch(first.clone());
    let first_model = fixture.facade.next_host_requests(8).remove(0);
    let HostRequest::ExecuteAgentModelTurn {
        execution: first_execution,
    } = &first_model.request
    else {
        panic!("expected first model request");
    };
    fixture.facade.record_host_observation(observe(
        &first_model,
        HostObservation::AgentModelCompleted {
            turn_id: first_execution.turn_id,
            model_fence_id: first_execution.model_fence_id,
            assistant_text: "Architecture matters because boundaries preserve options.".to_owned(),
            proposed_tool_call: None,
            usage: Some(AgentModelUsageObservation {
                prompt_tokens: 120,
                completion_tokens: 18,
                cached_prompt_tokens: Some(40),
            }),
        },
    ));
    let conversation_id = ConversationId::from_bytes(first.command_id.into_bytes());

    fixture.facade.dispatch(start_command(
        5,
        Some(conversation_id),
        "What did you just say?",
    ));
    let second_model = fixture.facade.next_host_requests(8).remove(0);
    let HostRequest::ExecuteAgentModelTurn { execution } = second_model.request else {
        panic!("expected follow-up model request");
    };

    assert_eq!(
        execution.messages,
        vec![
            AgentMessageProjection {
                role: AgentMessageRole::User,
                content: "Save architecture matters as a note".to_owned(),
            },
            AgentMessageProjection {
                role: AgentMessageRole::Assistant,
                content: "Architecture matters because boundaries preserve options.".to_owned(),
            },
            AgentMessageProjection {
                role: AgentMessageRole::User,
                content: "What did you just say?".to_owned(),
            },
        ]
    );
    let Projection::AgentConversation { value } = fixture
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::AgentConversation { conversation_id },
            offset: 0,
            max_items: 10,
        })
        .projection
    else {
        panic!("expected conversation projection");
    };
    assert_eq!(value.turns.len(), 2);
    assert_eq!(value.turns[0].messages.len(), 1);
    assert_eq!(value.turns[0].messages[0].content, "What did you just say?");
    assert_eq!(value.turns[1].model_usage.len(), 1);
    assert_eq!(value.turns[1].model_usage[0].prompt_tokens, 120);
}

fn start_command(
    id: u64,
    conversation_id: Option<ConversationId>,
    user_input: &str,
) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(101, id),
        cancellation_id: CancellationId::from_parts(102, id),
        expected_revision: None,
        command: ApplicationCommand::StartAgentTurn {
            conversation_id,
            user_input: user_input.to_owned(),
            model_reference: "openrouter/test".to_owned(),
            available_tools: vec![AgentToolName::CreateNote],
        },
    }
}

fn observe(request: &HostRequestEnvelope, observation: HostObservation) -> HostObservationEnvelope {
    HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: 1,
        observed_at: UnixTimestampMilliseconds::new(1_900_000_000_000),
        observation,
    }
}
