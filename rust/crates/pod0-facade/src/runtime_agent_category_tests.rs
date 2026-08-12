use crate::runtime_agent_modules::test_support::{
    next_leased_agent_request, record_leased_agent_observation, start_command,
};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn advertised_category_write_uses_the_durable_rust_handoff() {
    let fixture = PlaybackFixture::new();
    let start = start_command(51);
    fixture.facade.dispatch(start);
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
            assistant_text: "I'll create that category.".to_owned(),
            proposed_tool_call: Some(AgentModelToolCallObservation {
                provider_call_id: "category-call".to_owned(),
                tool_name: "write_category".to_owned(),
                arguments_json: r##"{"name":"Research","description":"Episodes worth revisiting.","color_hex":"#445566","delete":false}"##.to_owned(),
            }),
            usage: None,
        },
    );

    let approval = next_leased_agent_request(&fixture.facade);
    let HostRequest::PresentAgentApproval { approval: request } = &approval.request.request else {
        panic!("expected approval request");
    };
    assert!(matches!(
        record_leased_agent_observation(
            &fixture.facade,
            &approval,
            HostObservation::AgentApprovalObserved {
                turn_id: request.turn_id,
                proposal_id: request.proposal.proposal_id,
                proposal_digest: request.proposal.proposal_digest,
                decision: AgentApprovalDecision::Approve,
            },
        ),
        HostObservationReceipt::Persisted { .. }
    ));

    let store = pod0_storage::LibraryStore::open_authoritative(&fixture.target).unwrap();
    let snapshot = store.category_snapshot().unwrap();
    assert_eq!(snapshot.categories.len(), 1);
    assert_eq!(snapshot.categories[0].name, "Research");

    let continuation = next_leased_agent_request(&fixture.facade);
    let HostRequest::ExecuteAgentModelTurn { execution } = continuation.request.request else {
        panic!("expected continuation");
    };
    assert!(execution.messages.iter().any(|message| {
        message.role == AgentMessageRole::Tool && message.content.contains("category_id")
    }));

    let connection = rusqlite::Connection::open(&fixture.target).unwrap();
    let transitions: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_activity_facts WHERE fact_code=2 AND payload_json LIKE '%CategoryChanged%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(transitions >= 1);
}

#[test]
fn missing_category_is_durably_rejected_and_consumed() {
    let fixture = PlaybackFixture::new();
    fixture.facade.dispatch(start_command(52));
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
            assistant_text: "I'll rename it.".to_owned(),
            proposed_tool_call: Some(AgentModelToolCallObservation {
                provider_call_id: "missing-category-call".to_owned(),
                tool_name: "write_category".to_owned(),
                arguments_json: r#"{"category_id":"eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee","name":"Missing"}"#.to_owned(),
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

    assert!(
        pod0_storage::LibraryStore::open_authoritative(&fixture.target)
            .unwrap()
            .pending_internal_commands(20)
            .unwrap()
            .is_empty()
    );
    let connection = rusqlite::Connection::open(&fixture.target).unwrap();
    let rejected: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_activity_facts WHERE fact_code=1 AND payload_json LIKE '%MissingSubject%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(rejected >= 1);
    assert!(
        pod0_storage::LibraryStore::open_authoritative(&fixture.target)
            .unwrap()
            .category_snapshot()
            .unwrap()
            .categories
            .is_empty()
    );
}
