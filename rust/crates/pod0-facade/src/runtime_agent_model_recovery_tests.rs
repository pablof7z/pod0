use super::*;

#[test]
fn expired_model_attempt_becomes_ambiguous_and_rejects_its_late_callback() {
    let fixture = PlaybackFixture::new();
    let start = start_command(3);
    fixture.facade.dispatch(start.clone());
    let model = next_leased_agent_request(&fixture.facade);
    let HostRequest::ExecuteAgentModelTurn { execution } = &model.request.request else {
        panic!("expected model request");
    };

    fixture.facade.state().set_clock(std::sync::Arc::new(
        AgentRecoveryClock(model.lease.expires_at.value + 1),
    ));
    assert!(fixture.facade.next_leased_host_requests(1).is_empty());
    assert_eq!(
        turn(&fixture.facade, start.command_id).stage,
        AgentTurnStage::OutcomeAmbiguous
    );

    let receipt = fixture.facade.record_leased_host_observation(
        LeasedHostObservationEnvelope {
            lease: model.lease,
            observation: HostObservationEnvelope {
                request_id: model.request.request_id,
                cancellation_id: model.request.cancellation_id,
                observed_request_revision: model.request.issued_revision,
                sequence_number: 1,
                observed_at: UnixTimestampMilliseconds::new(model.lease.expires_at.value + 1),
                observation: HostObservation::AgentModelCompleted {
                    turn_id: execution.turn_id,
                    model_fence_id: execution.model_fence_id,
                    assistant_text: "This result arrived after recovery fenced it.".into(),
                    proposed_tool_call: None,
                    usage: None,
                },
            },
        },
    );
    assert!(matches!(receipt, HostObservationReceipt::Rejected { .. }));
    assert_eq!(
        turn(&fixture.facade, start.command_id).stage,
        AgentTurnStage::OutcomeAmbiguous
    );
}
