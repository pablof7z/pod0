use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

fn start_command(id: u64, tools: Vec<AgentToolName>) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(101, id),
        cancellation_id: CancellationId::from_parts(102, id),
        expected_revision: None,
        command: ApplicationCommand::StartAgentTurn {
            conversation_id: None,
            user_input: "Save architecture matters as a note".to_owned(),
            model_reference: "openrouter/test".to_owned(),
            available_tools: tools,
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
    assert!(value.failure.is_none());
    value.turns.into_iter().next().expect("turn must exist")
}

#[test]
fn note_action_requires_exact_approval_and_commits_once_in_rust() {
    let fixture = PlaybackFixture::new();
    let start = start_command(1, vec![AgentToolName::CreateNote]);
    fixture.facade.dispatch(start.clone());
    let model = fixture.facade.next_host_requests(8).remove(0);
    let HostRequest::ExecuteAgentModelTurn { execution } = &model.request else {
        panic!("expected model request");
    };
    let receipt = fixture.facade.record_host_observation(observe(
        &model,
        HostObservation::AgentModelCompleted {
            turn_id: execution.turn_id,
            model_fence_id: execution.model_fence_id,
            assistant_text: "I'll save that.".to_owned(),
            proposed_tool_call: Some(AgentModelToolCallObservation {
                provider_call_id: "note-call".to_owned(),
                tool_name: "create_note".to_owned(),
                arguments_json: r#"{"text":"Architecture matters"}"#.to_owned(),
            }),
            usage: None,
        },
    ));
    assert!(matches!(receipt, HostObservationReceipt::Persisted { .. }));

    let approval = fixture.facade.next_host_requests(8).remove(0);
    let HostRequest::PresentAgentApproval { approval: request } = &approval.request else {
        panic!("expected approval request");
    };
    let stale = fixture.facade.record_host_observation(observe(
        &approval,
        HostObservation::AgentApprovalObserved {
            turn_id: request.turn_id,
            proposal_id: request.proposal.proposal_id,
            proposal_digest: ContentDigest::from_bytes([77; 32]),
            approved: true,
        },
    ));
    assert!(matches!(
        stale,
        HostObservationReceipt::Rejected {
            reason: HostObservationRejection::MismatchedPayload,
            ..
        }
    ));
    assert!(
        fixture
            .facade
            .snapshot(ProjectionRequest {
                scope: ProjectionScope::Notes {
                    scope: NoteProjectionScope::All,
                },
                offset: 0,
                max_items: 10,
            })
            .projection
            .notes()
            .is_empty()
    );

    let approved = observe(
        &approval,
        HostObservation::AgentApprovalObserved {
            turn_id: request.turn_id,
            proposal_id: request.proposal.proposal_id,
            proposal_digest: request.proposal.proposal_digest,
            approved: true,
        },
    );
    assert!(matches!(
        fixture.facade.record_host_observation(approved.clone()),
        HostObservationReceipt::Persisted { .. }
    ));
    assert_eq!(
        turn(&fixture.facade, start.command_id).stage,
        AgentTurnStage::AwaitingModel
    );
    let Projection::Notes { value } = fixture
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Notes {
                scope: NoteProjectionScope::All,
            },
            offset: 0,
            max_items: 10,
        })
        .projection
    else {
        panic!("expected notes");
    };
    assert_eq!(value.notes.len(), 1);
    assert_eq!(value.notes[0].text, "Architecture matters");

    let continuation = fixture.facade.next_host_requests(8).remove(0);
    let HostRequest::ExecuteAgentModelTurn { execution } = &continuation.request else {
        panic!("expected final model continuation");
    };
    assert!(execution.available_tools.is_empty());
    assert!(execution.messages.iter().any(|message| {
        message.role == AgentMessageRole::Tool && message.content.contains("note_id")
    }));
    fixture.facade.record_host_observation(observe(
        &continuation,
        HostObservation::AgentModelCompleted {
            turn_id: execution.turn_id,
            model_fence_id: execution.model_fence_id,
            assistant_text: "Saved that note.".to_owned(),
            proposed_tool_call: None,
            usage: None,
        },
    ));
    assert_eq!(
        turn(&fixture.facade, start.command_id).stage,
        AgentTurnStage::Completed
    );
    assert_eq!(
        turn(&fixture.facade, start.command_id)
            .messages
            .last()
            .unwrap()
            .content,
        "Saved that note."
    );

    let _ = fixture.facade.record_host_observation(approved);
    let Projection::Notes { value } = fixture
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Notes {
                scope: NoteProjectionScope::All,
            },
            offset: 0,
            max_items: 10,
        })
        .projection
    else {
        panic!("expected notes");
    };
    assert_eq!(value.notes.len(), 1);
}

#[test]
fn native_action_is_fenced_and_restart_never_blindly_replays_it() {
    let fixture = PlaybackFixture::new();
    let start = start_command(2, vec![AgentToolName::PlayEpisode]);
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
            assistant_text: "Playing it.".to_owned(),
            proposed_tool_call: Some(AgentModelToolCallObservation {
                provider_call_id: "play-call".to_owned(),
                tool_name: "play_episode".to_owned(),
                arguments_json: format!(
                    r#"{{"episode_id":"{}","queue_position":"next"}}"#,
                    uuid_string(fixture.episode_id.into_bytes())
                ),
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
    assert!(matches!(
        capability.request,
        HostRequest::ExecuteAgentCapability { .. }
    ));

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    assert_eq!(
        turn(&reopened, start.command_id).stage,
        AgentTurnStage::OutcomeAmbiguous
    );
    assert!(reopened.next_host_requests(8).is_empty());
}

#[test]
fn invalid_and_unavailable_actions_fail_before_any_capability_request() {
    let fixture = PlaybackFixture::new();
    let start = start_command(3, vec![AgentToolName::CreateNote]);
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
                provider_call_id: "unavailable-call".to_owned(),
                tool_name: "play_episode".to_owned(),
                arguments_json: format!(
                    r#"{{"episode_id":"{}","queue_position":"next"}}"#,
                    uuid_string(fixture.episode_id.into_bytes())
                ),
            }),
            usage: None,
        },
    ));
    assert_eq!(
        turn(&fixture.facade, start.command_id).stage,
        AgentTurnStage::Failed
    );
    assert!(fixture.facade.next_host_requests(8).is_empty());
}

fn uuid_string(bytes: [u8; 16]) -> String {
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

trait ProjectionNotes {
    fn notes(&self) -> &[NoteRecord];
}

impl ProjectionNotes for Projection {
    fn notes(&self) -> &[NoteRecord] {
        match self {
            Projection::Notes { value } => &value.notes,
            _ => panic!("expected notes projection"),
        }
    }
}
