use super::tests::{
    next_leased_agent_request, record_leased_agent_observation, start_command, uuid_string,
};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn play_episode_requires_approval_then_dispatches_the_exact_capability() {
    let fixture = PlaybackFixture::new();
    let start = start_command(3);
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
                provider_call_id: "play-call".to_owned(),
                tool_name: "play_episode".to_owned(),
                arguments_json: format!(
                    r#"{{"episode_id":"{}","queue_position":"next"}}"#,
                    uuid_string(fixture.episode_id.into_bytes())
                ),
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
    let HostRequest::ExecuteAgentCapability {
        capability: request,
    } = &capability.request.request
    else {
        panic!("expected play capability");
    };
    assert!(matches!(
        request.action,
        AgentToolAction::PlayEpisode {
            episode_id,
            placement: QueuePlacement::Next,
            ..
        } if episode_id == fixture.episode_id
    ));
    assert!(matches!(
        record_leased_agent_observation(
            &fixture.facade,
            &capability,
            HostObservation::Failed {
                code: HostFailureCode::PlatformFailure,
                safe_detail: Some("playback bridge failed".to_owned()),
            },
        ),
        HostObservationReceipt::Persisted { .. }
    ));
    let conversation_id = ConversationId::from_bytes(start.command_id.into_bytes());
    let Projection::AgentConversation { value } = fixture
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::AgentConversation { conversation_id },
            offset: 0,
            max_items: 10,
        })
        .projection
    else {
        panic!("expected agent conversation");
    };
    assert_eq!(value.turns[0].stage, AgentTurnStage::Failed);
}
