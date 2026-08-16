pub(crate) use super::test_support::{
    next_leased_agent_request, observe, record_leased_agent_observation, start_command, uuid_string,
};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

struct AgentRecoveryClock(i64);

impl pod0_application::Clock for AgentRecoveryClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0)
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
    let start = start_command(1);
    fixture.facade.dispatch(start.clone());
    let model = next_leased_agent_request(&fixture.facade);
    let HostRequest::ExecuteAgentModelTurn { execution } = &model.request.request else {
        panic!("expected model request");
    };
    let receipt = record_leased_agent_observation(
        &fixture.facade,
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
    );
    assert!(matches!(receipt, HostObservationReceipt::Persisted { .. }));

    let approval = next_leased_agent_request(&fixture.facade);
    let HostRequest::PresentAgentApproval { approval: request } = &approval.request.request else {
        panic!("expected approval request");
    };
    let stale = record_leased_agent_observation(
        &fixture.facade,
        &approval,
        HostObservation::AgentApprovalObserved {
            turn_id: request.turn_id,
            proposal_id: request.proposal.proposal_id,
            proposal_digest: ContentDigest::from_bytes([77; 32]),
            decision: AgentApprovalDecision::Approve,
        },
    );
    assert!(matches!(
        stale,
        HostObservationReceipt::Rejected {
            reason: HostObservationRejection::StaleWorkflow,
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

    let approval_observation = HostObservation::AgentApprovalObserved {
        turn_id: request.turn_id,
        proposal_id: request.proposal.proposal_id,
        proposal_digest: request.proposal.proposal_digest,
        decision: AgentApprovalDecision::Approve,
    };
    assert!(matches!(
        record_leased_agent_observation(&fixture.facade, &approval, approval_observation.clone()),
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
    let connection = rusqlite::Connection::open(&fixture.target).unwrap();
    let note_id = value.notes[0].note_id;
    let note_facts: i64 = connection
        .query_row(
            "SELECT count(*) FROM pod0_activity_facts WHERE subject_code=8 AND subject_id=?1",
            [note_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(note_facts >= 2);
    let consumed: i64 = connection
        .query_row(
            "SELECT count(*) FROM pod0_internal_command_intents WHERE subject_code=4
             AND subject_id=?1 AND state_code=2",
            [request.turn_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(consumed, 3);

    let continuation = next_leased_agent_request(&fixture.facade);
    let HostRequest::ExecuteAgentModelTurn { execution } = &continuation.request.request else {
        panic!("expected final model continuation");
    };
    assert!(execution.tool_definitions.is_empty());
    assert!(execution.messages.iter().any(|message| {
        message.role == AgentMessageRole::Tool && message.content.contains("note_id")
    }));
    record_leased_agent_observation(
        &fixture.facade,
        &continuation,
        HostObservation::AgentModelCompleted {
            turn_id: execution.turn_id,
            model_fence_id: execution.model_fence_id,
            assistant_text: "Saved that note.".to_owned(),
            proposed_tool_call: None,
            usage: None,
        },
    );
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

    let _ = record_leased_agent_observation(&fixture.facade, &approval, approval_observation);
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
    let start = start_command(2);
    fixture.facade.dispatch(start.clone());
    let model = next_leased_agent_request(&fixture.facade);
    let HostRequest::ExecuteAgentModelTurn { execution } = &model.request.request else {
        panic!("expected model request");
    };
    record_leased_agent_observation(
        &fixture.facade,
        &model,
        HostObservation::AgentModelCompleted {
            turn_id: execution.turn_id,
            model_fence_id: execution.model_fence_id,
            assistant_text: "Playing it.".to_owned(),
            proposed_tool_call: Some(AgentModelToolCallObservation {
                provider_call_id: "pause-call".to_owned(),
                tool_name: "pause_playback".to_owned(),
                arguments_json: "{}".to_owned(),
            }),
            usage: None,
        },
    );
    let approval = next_leased_agent_request(&fixture.facade);
    let HostRequest::PresentAgentApproval { approval: request } = &approval.request.request else {
        panic!("expected approval request");
    };
    record_leased_agent_observation(
        &fixture.facade,
        &approval,
        HostObservation::AgentApprovalObserved {
            turn_id: request.turn_id,
            proposal_id: request.proposal.proposal_id,
            proposal_digest: request.proposal.proposal_digest,
            decision: AgentApprovalDecision::Approve,
        },
    );
    let capability = next_leased_agent_request(&fixture.facade);
    assert!(matches!(
        capability.request.request,
        HostRequest::ExecuteAgentCapability { .. }
    ));

    let reopened = Pod0Facade::open_with_clock(
        fixture.target.to_string_lossy().into_owned(),
        std::sync::Arc::new(AgentRecoveryClock(capability.lease.expires_at.value + 1)),
    );
    assert!(reopened.next_leased_host_requests(1).is_empty());
    assert_eq!(
        turn(&reopened, start.command_id).stage,
        AgentTurnStage::OutcomeAmbiguous
    );
    assert!(reopened.next_leased_host_requests(1).is_empty());
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
