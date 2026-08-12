use super::recall_test_support::{
    approve_next, observe, propose_query, start_command, turn, uuid_string,
};
use super::tests::{next_leased_agent_request, record_leased_agent_observation};
use crate::runtime_recall_test_support::{RecallFixture, recall_test_embedding, record};
use crate::*;

#[test]
fn transcript_query_returns_exact_evidence_then_finishes_conversationally() {
    let fixture = RecallFixture::new(true);
    let start = start_command(301);
    fixture.base.facade.dispatch(start.clone());
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
            assistant_text: "I'll check the transcript.".to_owned(),
            proposed_tool_call: Some(AgentModelToolCallObservation {
                provider_call_id: "recall-call".to_owned(),
                tool_name: "query_transcripts".to_owned(),
                arguments_json: format!(
                    r#"{{"query":"habit cues","episode_id":"{}","limit":2}}"#,
                    uuid_string(fixture.base.episode_id.into_bytes())
                ),
            }),
            usage: None,
        },
    );
    approve_next(&fixture);
    let embed = fixture.base.facade.next_host_requests(1).remove(0);
    let HostRequest::EmbedRecallQuery { query_id, text, .. } = &embed.request else {
        panic!("expected shared recall embedding request");
    };
    assert_eq!(text, "habit cues");
    record(
        &fixture.base.facade,
        &embed,
        HostObservation::RecallQueryEmbedded {
            query_id: *query_id,
            embedding: RecallEmbeddingVector {
                values: recall_test_embedding(),
            },
        },
    );
    let rerank = fixture.base.facade.next_host_requests(1).remove(0);
    assert!(matches!(
        rerank.request,
        HostRequest::RerankRecallCandidates { .. }
    ));
    record(
        &fixture.base.facade,
        &rerank,
        HostObservation::Failed {
            code: HostFailureCode::ProviderUnavailable,
            safe_detail: None,
        },
    );
    let continuation = fixture.base.facade.next_host_requests(1).remove(0);
    let HostRequest::ExecuteAgentModelTurn { execution } = &continuation.request else {
        panic!("expected final model continuation");
    };
    assert!(execution.tool_definitions.is_empty());
    let evidence = execution
        .messages
        .iter()
        .find(|message| message.role == AgentMessageRole::Tool)
        .expect("recall tool evidence must be durable");
    assert!(evidence.content.contains(r#""status":"ready""#));
    assert!(evidence.content.contains(r#""timestamp":"0:10""#));
    assert!(evidence.content.contains("daily cues"));
    assert!(evidence.content.contains(r#""playable_reference""#));

    fixture.base.facade.record_host_observation(observe(
        &continuation,
        HostObservation::AgentModelCompleted {
            turn_id: execution.turn_id,
            model_fence_id: execution.model_fence_id,
            assistant_text: "At 0:10, the episode says daily cues make habits repeatable."
                .to_owned(),
            proposed_tool_call: None,
            usage: None,
        },
    ));
    let completed = turn(&fixture.base.facade, start.command_id);
    assert_eq!(completed.stage, AgentTurnStage::Completed);
    assert_eq!(completed.recall_evidence.len(), 2);
    assert_eq!(completed.recall_evidence[0].start_milliseconds, 10_000);
    assert!(completed.recall_evidence[0].excerpt.contains("daily cues"));
}
#[test]
fn transcript_query_reissues_safe_read_only_work_after_restart() {
    let fixture = RecallFixture::new(true);
    let start = start_command(302);
    fixture.base.facade.dispatch(start.clone());
    propose_query(&fixture);
    approve_next(&fixture);
    let first = fixture.base.facade.next_host_requests(1).remove(0);
    assert!(matches!(
        first.request,
        HostRequest::EmbedRecallQuery { .. }
    ));

    let reopened = Pod0Facade::open(fixture.base.target.to_string_lossy().into_owned()).unwrap();
    let recovered = reopened.next_host_requests(1).remove(0);
    assert!(matches!(
        recovered.request,
        HostRequest::EmbedRecallQuery { .. }
    ));
    assert_eq!(
        turn(&reopened, start.command_id).stage,
        AgentTurnStage::Executing
    );
}

#[test]
fn transcript_query_reports_provider_failure_for_conversational_recovery() {
    let fixture = RecallFixture::new(true);
    let start = start_command(303);
    fixture.base.facade.dispatch(start.clone());
    propose_query(&fixture);
    approve_next(&fixture);
    let embed = fixture.base.facade.next_host_requests(1).remove(0);
    record(
        &fixture.base.facade,
        &embed,
        HostObservation::Failed {
            code: HostFailureCode::ProviderUnavailable,
            safe_detail: None,
        },
    );

    let continuation = fixture.base.facade.next_host_requests(1).remove(0);
    let HostRequest::ExecuteAgentModelTurn { execution } = &continuation.request else {
        panic!("expected recovery model continuation");
    };
    let result = execution
        .messages
        .iter()
        .find(|message| message.role == AgentMessageRole::Tool)
        .expect("terminal failure result must remain available to the model");
    assert!(
        result
            .content
            .contains(r#""status":"provider_unavailable""#)
    );
    assert!(result.content.contains(r#""evidence":[]"#));
}

#[test]
fn cancelling_agent_turn_withdraws_recall_and_rejects_late_completion() {
    let fixture = RecallFixture::new(true);
    let start = start_command(304);
    fixture.base.facade.dispatch(start.clone());
    propose_query(&fixture);
    approve_next(&fixture);
    let embed = fixture.base.facade.next_host_requests(1).remove(0);
    let HostRequest::EmbedRecallQuery { query_id, .. } = &embed.request else {
        panic!("expected recall embedding request");
    };
    let query_id = *query_id;
    let active = turn(&fixture.base.facade, start.command_id);

    fixture.base.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(303, 304),
        cancellation_id: CancellationId::from_parts(304, 304),
        expected_revision: None,
        command: ApplicationCommand::CancelAgentTurn {
            turn_id: active.turn_id,
            expected_turn_revision: active.revision,
        },
    });
    assert_eq!(
        turn(&fixture.base.facade, start.command_id).stage,
        AgentTurnStage::Cancelled
    );

    let before = fixture.base.facade.snapshot(ProjectionRequest {
        scope: ProjectionScope::AgentConversation {
            conversation_id: active.conversation_id,
        },
        offset: 0,
        max_items: 10,
    });
    record(
        &fixture.base.facade,
        &embed,
        HostObservation::RecallQueryEmbedded {
            query_id,
            embedding: RecallEmbeddingVector {
                values: recall_test_embedding(),
            },
        },
    );
    let after = fixture.base.facade.snapshot(ProjectionRequest {
        scope: ProjectionScope::AgentConversation {
            conversation_id: active.conversation_id,
        },
        offset: 0,
        max_items: 10,
    });
    assert_eq!(after.state_revision, before.state_revision);
    assert!(fixture.base.facade.next_host_requests(1).is_empty());
}
