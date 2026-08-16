use super::tests::{next_leased_agent_request, record_leased_agent_observation};
use crate::runtime_recall_test_support::RecallFixture;
use crate::*;

pub(super) fn propose_query(fixture: &RecallFixture) {
    let model = next_leased_agent_request(&fixture.base.facade);
    let HostRequest::ExecuteAgentModelTurn { execution } = &model.request.request else {
        panic!("expected model request");
    };
    record_leased_agent_observation(
        &fixture.base.facade,
        &model,
        HostObservation::AgentModelCompleted {
            turn_id: execution.turn_id,
            model_fence_id: execution.model_fence_id,
            assistant_text: String::new(),
            proposed_tool_call: Some(AgentModelToolCallObservation {
                provider_call_id: "recall-call".to_owned(),
                tool_name: "query_transcripts".to_owned(),
                arguments_json: format!(
                    r#"{{"query":"habit cues","episode_id":"{}"}}"#,
                    uuid_string(fixture.base.episode_id.into_bytes())
                ),
            }),
            usage: None,
        },
    );
}

pub(super) fn approve_next(fixture: &RecallFixture) {
    let approval = next_leased_agent_request(&fixture.base.facade);
    let HostRequest::PresentAgentApproval { approval: request } = &approval.request.request else {
        panic!("expected approval request");
    };
    record_leased_agent_observation(
        &fixture.base.facade,
        &approval,
        HostObservation::AgentApprovalObserved {
            turn_id: request.turn_id,
            proposal_id: request.proposal.proposal_id,
            proposal_digest: request.proposal.proposal_digest,
            decision: AgentApprovalDecision::Approve,
        },
    );
}

pub(super) fn start_command(id: u64) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(301, id),
        cancellation_id: CancellationId::from_parts(302, id),
        expected_revision: None,
        command: ApplicationCommand::StartAgentTurn {
            conversation_id: None,
            user_input: "What did this episode say?".to_owned(),
            model_reference: "openrouter/test".to_owned(),
        },
    }
}

pub(super) fn turn(facade: &Pod0Facade, command_id: CommandId) -> AgentTurnProjection {
    let conversation_id = ConversationId::from_bytes(command_id.into_bytes());
    let Projection::AgentConversation { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::AgentConversation { conversation_id },
            offset: 0,
            max_items: 10,
        })
        .projection
    else {
        panic!("expected agent conversation");
    };
    value.turns.into_iter().next().expect("turn must exist")
}

pub(super) fn uuid_string(bytes: [u8; 16]) -> String {
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
