use pod0_application::{
    TranscriptCapabilityContext, TranscriptCapabilityObservation, TranscriptCapabilityRequest,
    TranscriptProvider, TranscriptWorkflowConfiguration, TranscriptWorkflowOrigin,
    TranscriptWorkflowProjection, TranscriptWorkflowStage,
};
use pod0_domain::{
    ContentDigest, TranscriptArtifactInput, TranscriptArtifactSegmentInput, TranscriptSource,
};

use crate::runtime_playback_test_support::{PlaybackFixture, library_request};
use crate::*;

#[test]
fn transcript_workflow_commits_indexes_and_survives_relaunch() {
    let fixture = PlaybackFixture::new();
    let command_id = CommandId::from_parts(70, 1);
    fixture.facade.dispatch(CommandEnvelope {
        command_id,
        cancellation_id: CancellationId::from_parts(70, 2),
        expected_revision: None,
        command: ApplicationCommand::EnsureTranscriptWorkflow {
            episode_id: fixture.episode_id,
            origin: TranscriptWorkflowOrigin::User,
            configuration: configuration(),
        },
    });

    let request = fixture
        .facade
        .next_host_requests(u16::MAX)
        .into_iter()
        .find(|request| {
            matches!(
                request.request,
                HostRequest::ExecuteTranscriptCapability { .. }
            )
        })
        .expect("transcript request");
    let HostRequest::ExecuteTranscriptCapability {
        capability: TranscriptCapabilityRequest::SubmitProvider { context, .. },
    } = &request.request
    else {
        panic!("expected provider submission");
    };
    let receipt = fixture
        .facade
        .record_host_observation(HostObservationEnvelope {
            request_id: request.request_id,
            cancellation_id: request.cancellation_id,
            observed_request_revision: request.issued_revision,
            sequence_number: 0,
            observed_at: UnixTimestampMilliseconds::new(1_900_000_000_000),
            observation: HostObservation::TranscriptCapabilityObserved {
                observation: TranscriptCapabilityObservation::Completed {
                    external_operation_id: None,
                    provider_status: Some("completed".into()),
                    artifact: transcript(context),
                },
            },
        });
    assert!(matches!(
        receipt,
        HostObservationReceipt::Persisted { terminal: true, .. }
    ));

    crate::runtime_recall_test_support::complete_evidence_embedding_requests(&fixture.facade);
    let projected = workflow(&fixture.facade, fixture.episode_id);
    assert_eq!(projected.stage, TranscriptWorkflowStage::Succeeded);
    assert!(projected.failure.is_none());
    let selected = transcript_summary(&fixture.facade, fixture.episode_id);
    assert_eq!(selected.source_revision, projected.source_revision);
    assert!(matches!(
        operation(&fixture.facade, command_id).stage,
        OperationStage::Succeeded
    ));

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    assert_eq!(
        workflow(&reopened, fixture.episode_id).stage,
        TranscriptWorkflowStage::Succeeded
    );
    assert_eq!(
        transcript_summary(&reopened, fixture.episode_id).transcript_version_id,
        selected.transcript_version_id
    );
    assert!(
        reopened
            .next_host_requests(u16::MAX)
            .into_iter()
            .all(|request| !matches!(
                request.request,
                HostRequest::ExecuteTranscriptCapability { .. }
            ))
    );
}

fn configuration() -> TranscriptWorkflowConfiguration {
    TranscriptWorkflowConfiguration {
        provider: TranscriptProvider::AssemblyAi,
        model: "universal-2".into(),
        local_audio_url: None,
        credential_available: true,
        auto_publisher_enabled: true,
        auto_provider_enabled: true,
    }
}

fn transcript(context: &TranscriptCapabilityContext) -> TranscriptArtifactInput {
    TranscriptArtifactInput {
        episode_id: context.episode_id,
        podcast_id: context.podcast_id,
        source_revision: context.source_revision.clone(),
        source: TranscriptSource::AssemblyAi,
        provider: Some("assemblyAI".into()),
        source_payload_digest: ContentDigest::from_bytes([0x71; 32]),
        language: "en-US".into(),
        generated_at: UnixTimestampMilliseconds::new(1_900_000_000_000),
        speakers: Vec::new(),
        segments: vec![TranscriptArtifactSegmentInput {
            text: "Calm by default, alive on demand.".into(),
            start_milliseconds: 0,
            end_milliseconds: 2_000,
            speaker_id: None,
            words: Vec::new(),
        }],
    }
}

fn workflow(facade: &Pod0Facade, episode_id: EpisodeId) -> TranscriptWorkflowProjection {
    let Projection::TranscriptWorkflows { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::TranscriptWorkflows {
                episode_id: Some(episode_id),
            },
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected transcript workflow projection");
    };
    value.workflows.into_iter().next().expect("workflow")
}

fn transcript_summary(facade: &Pod0Facade, episode_id: EpisodeId) -> TranscriptSummaryProjection {
    let Projection::Transcript { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Transcript {
                episode_id,
                scope: TranscriptProjectionScope::Summary,
            },
            offset: 0,
            max_items: 1,
        })
        .projection
    else {
        panic!("expected transcript projection");
    };
    value.summary.expect("selected transcript")
}

fn operation(facade: &Pod0Facade, command_id: CommandId) -> OperationProjection {
    let Projection::Library { value } = facade.snapshot(library_request()).projection else {
        panic!("expected library");
    };
    value
        .operations
        .into_iter()
        .find(|operation| operation.command_id == command_id)
        .expect("operation")
}
