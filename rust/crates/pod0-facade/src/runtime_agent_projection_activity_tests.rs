use super::tests::{next_leased_agent_request, record_leased_agent_observation, start_command};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn rust_projection_tool_runs_as_a_durable_internal_command_and_leased_continuation() {
    let fixture = PlaybackFixture::new();
    let start = start_command(9_007);
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
            assistant_text: String::new(),
            proposed_tool_call: Some(AgentModelToolCallObservation {
                provider_call_id: "list-podcasts".to_owned(),
                tool_name: "list_podcasts".to_owned(),
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

    let continuation = next_leased_agent_request(&fixture.facade);
    let HostRequest::ExecuteAgentModelTurn { execution } = &continuation.request.request else {
        panic!("expected leased continuation model request");
    };
    assert!(execution.tool_definitions.is_empty());
    assert!(execution.messages.iter().any(|message| {
        message.role == AgentMessageRole::Tool && message.content.contains("podcasts")
    }));
    assert!(fixture.facade.next_host_requests(8).is_empty());

    let connection = rusqlite::Connection::open(&fixture.target).unwrap();
    let consumed: i64 = connection
        .query_row(
            "SELECT count(*) FROM pod0_internal_command_intents WHERE subject_code=4
             AND subject_id=?1 AND state_code=2",
            [execution.turn_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(consumed, 2);
}
