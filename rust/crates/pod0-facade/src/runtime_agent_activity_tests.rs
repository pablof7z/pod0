use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn agent_model_call_is_authorized_leased_and_observed_across_restart() {
    let fixture = PlaybackFixture::new();
    let command = super::tests::start_command(9_001);
    fixture.facade.dispatch(command.clone());
    assert!(fixture.facade.next_host_requests(1).is_empty());

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let leased = reopened.next_leased_host_requests(1).remove(0);
    let HostRequest::ExecuteAgentModelTurn { execution } = &leased.request.request else {
        panic!("expected leased agent model request");
    };
    assert_eq!(
        execution.turn_id.into_bytes(),
        command.command_id.into_bytes()
    );
    let mut observation = super::tests::observe(
        &leased.request,
        HostObservation::AgentModelCompleted {
            turn_id: execution.turn_id,
            model_fence_id: execution.model_fence_id,
            assistant_text: "Done.".to_owned(),
            proposed_tool_call: None,
            usage: None,
        },
    );
    observation.observed_at = leased.lease.expires_at;
    let receipt = reopened.record_leased_host_observation(LeasedHostObservationEnvelope {
        lease: leased.lease,
        observation,
    });
    assert!(
        matches!(receipt, HostObservationReceipt::Persisted { .. }),
        "unexpected receipt: {receipt:?}"
    );
    let conversation_id = ConversationId::from_bytes(command.command_id.into_bytes());
    let Projection::AgentConversation { value } = reopened
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::AgentConversation { conversation_id },
            offset: 0,
            max_items: 10,
        })
        .projection
    else {
        panic!("expected agent conversation");
    };
    assert_eq!(value.turns[0].stage, AgentTurnStage::Completed);
    assert!(reopened.next_leased_host_requests(1).is_empty());
}

#[test]
fn generic_failure_is_routed_by_persisted_model_effect_kind() {
    let fixture = PlaybackFixture::new();
    let command = super::tests::start_command(9_004);
    fixture.facade.dispatch(command.clone());
    let model = fixture.facade.next_leased_host_requests(1).remove(0);
    assert!(matches!(
        model.request.request,
        HostRequest::ExecuteAgentModelTurn { .. }
    ));
    let mut observation = super::tests::observe(
        &model.request,
        HostObservation::Failed {
            code: HostFailureCode::TimedOut,
            safe_detail: Some("model timed out".to_owned()),
        },
    );
    observation.observed_at = model.lease.expires_at;
    assert!(matches!(
        fixture
            .facade
            .record_leased_host_observation(LeasedHostObservationEnvelope {
                lease: model.lease,
                observation,
            }),
        HostObservationReceipt::Persisted { .. }
    ));
    assert_eq!(
        turn(&fixture.facade, command.command_id).stage,
        AgentTurnStage::Failed
    );
}

#[test]
fn agent_approval_is_a_separate_causal_lease_and_denial_is_terminal() {
    let fixture = PlaybackFixture::new();
    let command = super::tests::start_command(9_002);
    fixture.facade.dispatch(command.clone());
    let model = fixture.facade.next_leased_host_requests(1).remove(0);
    let HostRequest::ExecuteAgentModelTurn { execution } = &model.request.request else {
        panic!("expected leased model request");
    };
    let mut model_observation = super::tests::observe(
        &model.request,
        HostObservation::AgentModelCompleted {
            turn_id: execution.turn_id,
            model_fence_id: execution.model_fence_id,
            assistant_text: "I can save that.".to_owned(),
            proposed_tool_call: Some(AgentModelToolCallObservation {
                provider_call_id: "save-note".to_owned(),
                tool_name: "create_note".to_owned(),
                arguments_json: r#"{"text":"Architecture matters"}"#.to_owned(),
            }),
            usage: None,
        },
    );
    model_observation.observed_at = model.lease.expires_at;
    assert!(matches!(
        fixture
            .facade
            .record_leased_host_observation(LeasedHostObservationEnvelope {
                lease: model.lease,
                observation: model_observation,
            }),
        HostObservationReceipt::Persisted { .. }
    ));

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let approval = reopened.next_leased_host_requests(1).remove(0);
    let HostRequest::PresentAgentApproval { approval: request } = &approval.request.request else {
        panic!("expected leased approval request");
    };
    let mut approval_observation = super::tests::observe(
        &approval.request,
        HostObservation::AgentApprovalObserved {
            turn_id: request.turn_id,
            proposal_id: request.proposal.proposal_id,
            proposal_digest: request.proposal.proposal_digest,
            decision: AgentApprovalDecision::Deny,
        },
    );
    approval_observation.observed_at = approval.lease.expires_at;
    assert!(matches!(
        reopened.record_leased_host_observation(LeasedHostObservationEnvelope {
            lease: approval.lease,
            observation: approval_observation,
        }),
        HostObservationReceipt::Persisted { .. }
    ));
    assert_eq!(
        turn(&reopened, command.command_id).stage,
        AgentTurnStage::Denied
    );
}

#[test]
fn approval_authorizes_one_durable_execution_command_before_tool_dispatch() {
    let fixture = PlaybackFixture::new();
    let command = super::tests::start_command(9_003);
    fixture.facade.dispatch(command.clone());
    let model = fixture.facade.next_leased_host_requests(1).remove(0);
    let HostRequest::ExecuteAgentModelTurn { execution } = &model.request.request else {
        panic!("expected leased model request");
    };
    let mut observation = super::tests::observe(
        &model.request,
        HostObservation::AgentModelCompleted {
            turn_id: execution.turn_id,
            model_fence_id: execution.model_fence_id,
            assistant_text: "I can pause it.".to_owned(),
            proposed_tool_call: Some(AgentModelToolCallObservation {
                provider_call_id: "pause".to_owned(),
                tool_name: "pause_playback".to_owned(),
                arguments_json: "{}".to_owned(),
            }),
            usage: None,
        },
    );
    observation.observed_at = model.lease.expires_at;
    fixture
        .facade
        .record_leased_host_observation(LeasedHostObservationEnvelope {
            lease: model.lease,
            observation,
        });
    let approval = fixture.facade.next_leased_host_requests(1).remove(0);
    let HostRequest::PresentAgentApproval { approval: request } = &approval.request.request else {
        panic!("expected leased approval request");
    };
    let mut observation = super::tests::observe(
        &approval.request,
        HostObservation::AgentApprovalObserved {
            turn_id: request.turn_id,
            proposal_id: request.proposal.proposal_id,
            proposal_digest: request.proposal.proposal_digest,
            decision: AgentApprovalDecision::Approve,
        },
    );
    observation.observed_at = approval.lease.expires_at;
    assert!(matches!(
        fixture
            .facade
            .record_leased_host_observation(LeasedHostObservationEnvelope {
                lease: approval.lease,
                observation,
            }),
        HostObservationReceipt::Persisted { .. }
    ));
    assert_eq!(
        turn(&fixture.facade, command.command_id).stage,
        AgentTurnStage::Executing
    );
    assert!(matches!(
        fixture
            .facade
            .next_leased_host_requests(1)
            .remove(0)
            .request
            .request,
        HostRequest::ExecuteAgentCapability { .. }
    ));
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
        panic!("expected agent conversation");
    };
    value.turns.into_iter().next().expect("turn exists")
}
