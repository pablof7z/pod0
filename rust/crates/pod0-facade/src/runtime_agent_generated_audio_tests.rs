use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

pub(crate) fn observe(
    request: &HostRequestEnvelope,
    observation: HostObservation,
) -> HostObservationEnvelope {
    HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: 1,
        observed_at: UnixTimestampMilliseconds::new(1_900_000_000_000),
        observation,
    }
}

pub(crate) fn start(fixture: &PlaybackFixture, id: u64) -> (CommandEnvelope, HostRequestEnvelope) {
    let command = CommandEnvelope {
        command_id: CommandId::from_parts(301, id),
        cancellation_id: CancellationId::from_parts(302, id),
        expected_revision: None,
        command: ApplicationCommand::StartAgentTurn {
            conversation_id: None,
            user_input: "Turn this into a short briefing".into(),
            model_reference: "openrouter/test".into(),
        },
    };
    fixture.facade.dispatch(command.clone());
    let model = fixture.facade.next_host_requests(8).remove(0);
    let HostRequest::ExecuteAgentModelTurn { execution } = &model.request else {
        panic!("expected model request");
    };
    fixture.facade.record_host_observation(observe(
        &model,
        HostObservation::AgentModelCompleted {
            turn_id: execution.turn_id,
            model_fence_id: execution.model_fence_id,
            assistant_text: "I can create that briefing.".into(),
            proposed_tool_call: Some(AgentModelToolCallObservation {
                provider_call_id: "tts-call".into(),
                tool_name: "generate_tts_episode".into(),
                arguments_json: r#"{"title":"Calm Briefing","script":"One useful idea for today.","voice_id":"calm"}"#.into(),
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
    (command, fixture.facade.next_host_requests(8).remove(0))
}

pub(crate) fn generated_episode(facade: &Pod0Facade) -> EpisodeRecord {
    let Projection::Library { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Library,
            offset: 0,
            max_items: 100,
        })
        .projection
    else {
        panic!("expected library");
    };
    value
        .episodes
        .into_iter()
        .find(|episode| episode.generated_audio.is_some())
        .expect("generated episode")
}

#[test]
fn generated_audio_evidence_atomically_commits_a_restart_safe_episode() {
    let fixture = PlaybackFixture::new();
    let (command, capability) = start(&fixture, 1);
    let HostRequest::ExecuteAgentCapability {
        capability: request,
    } = &capability.request
    else {
        panic!("expected capability request");
    };
    assert_eq!(
        request.execution_mode,
        AgentCapabilityExecutionMode::Perform
    );
    let target = request.generated_audio_target.expect("generated target");
    let evidence = AgentGeneratedAudioEvidence {
        artifact_id: target.artifact_id,
        file_url: "file:///private/agent/calm-briefing.mp3".into(),
        media_type: "audio/mpeg".into(),
        byte_count: 4_096,
        content_digest: ContentDigest::from_bytes([31; 32]),
        duration_milliseconds: Some(30_000),
    };
    assert!(matches!(
        fixture.facade.record_host_observation(observe(
            &capability,
            HostObservation::AgentCapabilityObserved {
                turn_id: request.turn_id,
                proposal_id: request.proposal_id,
                execution_fence_id: request.execution_fence_id,
                outcome: AgentCapabilityOutcome::GeneratedAudioStaged {
                    evidence: evidence.clone(),
                },
            },
        )),
        HostObservationReceipt::Persisted { .. }
    ));

    let episode = generated_episode(&fixture.facade);
    let provenance = episode.generated_audio.unwrap();
    assert_eq!(provenance.artifact_id, target.artifact_id);
    assert_eq!(provenance.media_content_digest, evidence.content_digest);
    assert_eq!(provenance.media_byte_count, evidence.byte_count);
    assert_eq!(episode.title, "Calm Briefing");

    let continuation = fixture.facade.next_host_requests(8).remove(0);
    let HostRequest::ExecuteAgentModelTurn { execution } = &continuation.request else {
        panic!("expected final model continuation");
    };
    assert!(execution.tool_definitions.is_empty());
    assert!(
        execution
            .messages
            .iter()
            .any(|message| message.content.contains("generated_episode"))
    );

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let reopened_episode = generated_episode(&reopened);
    assert_eq!(reopened_episode.episode_id, episode.episode_id);
    let conversation_id = ConversationId::from_bytes(command.command_id.into_bytes());
    let Projection::AgentConversation { value } = reopened
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::AgentConversation { conversation_id },
            offset: 0,
            max_items: 10,
        })
        .projection
    else {
        panic!("expected conversation");
    };
    assert_eq!(value.turns[0].stage, AgentTurnStage::AwaitingModel);
}

#[test]
fn restart_requeues_generated_audio_only_for_existing_artifact_recovery() {
    let fixture = PlaybackFixture::new();
    let (_, capability) = start(&fixture, 2);
    let HostRequest::ExecuteAgentCapability {
        capability: original,
    } = &capability.request
    else {
        panic!("expected capability request");
    };
    assert_eq!(
        original.execution_mode,
        AgentCapabilityExecutionMode::Perform
    );

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let recovered = reopened.next_host_requests(8).remove(0);
    let HostRequest::ExecuteAgentCapability {
        capability: request,
    } = recovered.request
    else {
        panic!("expected recovered capability");
    };
    assert_eq!(
        request.execution_mode,
        AgentCapabilityExecutionMode::RecoverExisting
    );
    assert_eq!(
        request.generated_audio_target,
        original.generated_audio_target
    );
    assert_eq!(request.proposal_id, original.proposal_id);
}
