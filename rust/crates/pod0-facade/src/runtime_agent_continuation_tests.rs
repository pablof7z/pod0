use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

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

fn turn(facade: &Pod0Facade, command_id: CommandId) -> AgentTurnProjection {
    let conversation_id = ConversationId::from_bytes(command_id.into_bytes());
    let Projection::AgentConversation { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::AgentConversation { conversation_id },
            offset: 0,
            max_items: 10,
        })
        .projection
    else {
        panic!("expected agent conversation projection");
    };
    value.turns.into_iter().next().expect("turn must exist")
}

#[test]
fn successful_native_action_queues_one_tool_free_final_answer() {
    let fixture = PlaybackFixture::new();
    let start = CommandEnvelope {
        command_id: CommandId::from_parts(201, 4),
        cancellation_id: CancellationId::from_parts(202, 4),
        expected_revision: None,
        command: ApplicationCommand::StartAgentTurn {
            conversation_id: None,
            user_input: "Pause playback".to_owned(),
            model_reference: "openrouter/test".to_owned(),
            available_tools: vec![AgentToolName::PausePlayback],
        },
    };
    fixture.facade.dispatch(start.clone());
    let model = fixture.facade.next_host_requests(8).remove(0);
    let HostRequest::ExecuteAgentModelTurn { execution } = &model.request else {
        panic!("expected model request");
    };
    fixture.facade.record_host_observation(observe(
        &model,
        HostObservation::AgentModelCompleted {
            turn_id: execution.turn_id,
            model_fence_id: execution.model_fence_id,
            assistant_text: String::new(),
            proposed_tool_call: Some(AgentModelToolCallObservation {
                provider_call_id: "pause-call".to_owned(),
                tool_name: "pause_playback".to_owned(),
                arguments_json: "{}".to_owned(),
            }),
            usage: None,
        },
    ));
    let approval = fixture.facade.next_host_requests(8).remove(0);
    let HostRequest::PresentAgentApproval { approval: request } = &approval.request else {
        panic!("expected approval request");
    };
    fixture.facade.record_host_observation(observe(
        &approval,
        HostObservation::AgentApprovalObserved {
            turn_id: request.turn_id,
            proposal_id: request.proposal.proposal_id,
            proposal_digest: request.proposal.proposal_digest,
            approved: true,
        },
    ));
    let capability = fixture.facade.next_host_requests(8).remove(0);
    let HostRequest::ExecuteAgentCapability {
        capability: request,
    } = &capability.request
    else {
        panic!("expected native capability request");
    };
    fixture.facade.record_host_observation(observe(
        &capability,
        HostObservation::AgentCapabilityObserved {
            turn_id: request.turn_id,
            proposal_id: request.proposal_id,
            execution_fence_id: request.execution_fence_id,
            outcome: AgentCapabilityOutcome::Succeeded {
                bounded_result: r#"{"paused":true}"#.to_owned(),
            },
        },
    ));

    let continuation = fixture.facade.next_host_requests(8).remove(0);
    let HostRequest::ExecuteAgentModelTurn { execution } = &continuation.request else {
        panic!("expected final model continuation");
    };
    assert!(execution.available_tools.is_empty());
    assert!(execution.messages.iter().any(|message| {
        message.role == AgentMessageRole::Tool && message.content.contains("paused")
    }));
    assert_eq!(
        turn(&fixture.facade, start.command_id).stage,
        AgentTurnStage::AwaitingModel
    );
}
