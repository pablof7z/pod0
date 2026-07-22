use std::sync::Arc;

use pod0_application::{
    Clock, TranscriptCapabilityObservation, TranscriptCapabilityRequest, TranscriptProvider,
    TranscriptWorkflowConfiguration, TranscriptWorkflowOrigin, TranscriptWorkflowStage,
};
use pod0_domain::{ContentDigest, TranscriptSource};

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn accepted_provider_operation_recovers_once_after_a_durable_wake() {
    let fixture = PlaybackFixture::new();
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_900_000_000_000)));
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(72, 1),
        cancellation_id: CancellationId::from_parts(72, 2),
        expected_revision: None,
        command: ApplicationCommand::EnsureTranscriptWorkflow {
            episode_id: fixture.episode_id,
            origin: TranscriptWorkflowOrigin::User,
            configuration: configuration(),
        },
    });
    let submission = transcript_request(&fixture.facade, "submission");
    assert!(matches!(
        submission.request,
        HostRequest::ExecuteTranscriptCapability {
            capability: TranscriptCapabilityRequest::SubmitProvider { .. }
        }
    ));
    record(
        &fixture.facade,
        &submission,
        HostObservation::TranscriptCapabilityObserved {
            observation: TranscriptCapabilityObservation::ProviderAccepted {
                external_operation_id: "assembly-job-1".into(),
                provider_status: Some("queued".into()),
            },
        },
    );

    let wake = fixture
        .facade
        .next_host_requests(u16::MAX)
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::ScheduleCoreWake { .. }))
        .expect("durable provider recovery wake");
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_900_000_006_000)));
    let HostRequest::ScheduleCoreWake { reason, .. } = wake.request else {
        unreachable!()
    };
    record(
        &fixture.facade,
        &wake,
        HostObservation::CoreWakeReached { reason },
    );
    let recovery = transcript_request(&fixture.facade, "recovery");
    let HostRequest::ExecuteTranscriptCapability {
        capability:
            TranscriptCapabilityRequest::RecoverProvider {
                context,
                external_operation_id,
                ..
            },
    } = &recovery.request
    else {
        panic!("expected provider recovery");
    };
    assert_eq!(external_operation_id, "assembly-job-1");
    record(
        &fixture.facade,
        &recovery,
        HostObservation::TranscriptCapabilityObserved {
            observation: TranscriptCapabilityObservation::Completed {
                external_operation_id: Some(external_operation_id.clone()),
                provider_status: Some("completed".into()),
                artifact: transcript(context),
            },
        },
    );
    crate::runtime_recall_test_support::complete_evidence_embedding_requests(&fixture.facade);
    assert_eq!(workflow_stage(&fixture), TranscriptWorkflowStage::Succeeded);
}

#[test]
fn cancellation_withdraws_an_unsubmitted_transcript_request_durably() {
    let fixture = PlaybackFixture::new();
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(73, 1),
        cancellation_id: CancellationId::from_parts(73, 2),
        expected_revision: None,
        command: ApplicationCommand::EnsureTranscriptWorkflow {
            episode_id: fixture.episode_id,
            origin: TranscriptWorkflowOrigin::User,
            configuration: configuration(),
        },
    });
    let revision = workflow(&fixture).workflow_revision;
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(73, 3),
        cancellation_id: CancellationId::from_parts(73, 4),
        expected_revision: None,
        command: ApplicationCommand::CancelTranscriptWorkflow {
            episode_id: fixture.episode_id,
            expected_workflow_revision: revision,
        },
    });
    assert_eq!(workflow_stage(&fixture), TranscriptWorkflowStage::Cancelled);
    assert!(
        fixture
            .facade
            .next_host_requests(u16::MAX)
            .into_iter()
            .all(|request| !matches!(
                request.request,
                HostRequest::ExecuteTranscriptCapability { .. }
            ))
    );
    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    assert_eq!(
        workflow_for(&reopened, fixture.episode_id).stage,
        TranscriptWorkflowStage::Cancelled
    );
}

fn configuration() -> TranscriptWorkflowConfiguration {
    TranscriptWorkflowConfiguration {
        provider: TranscriptProvider::AssemblyAi,
        model: "universal-2".into(),
        local_audio_url: None,
        credential_available: true,
        auto_publisher_enabled: false,
        auto_provider_enabled: true,
    }
}

fn transcript_request(facade: &Pod0Facade, phase: &str) -> HostRequestEnvelope {
    let requests = facade.next_host_requests(u16::MAX);
    requests
        .iter()
        .find(|request| {
            matches!(
                request.request,
                HostRequest::ExecuteTranscriptCapability { .. }
            )
        })
        .cloned()
        .unwrap_or_else(|| panic!("{phase} transcript capability was not emitted: {requests:?}"))
}

fn record(facade: &Pod0Facade, request: &HostRequestEnvelope, observation: HostObservation) {
    assert!(matches!(
        facade.record_host_observation(HostObservationEnvelope {
            request_id: request.request_id,
            cancellation_id: request.cancellation_id,
            observed_request_revision: request.issued_revision,
            sequence_number: 0,
            observed_at: UnixTimestampMilliseconds::new(1_900_000_003_000),
            observation,
        }),
        HostObservationReceipt::Persisted { .. } | HostObservationReceipt::AcceptedTransient { .. }
    ));
}

fn transcript(context: &pod0_application::TranscriptCapabilityContext) -> TranscriptArtifactInput {
    TranscriptArtifactInput {
        episode_id: context.episode_id,
        podcast_id: context.podcast_id,
        source_revision: context.source_revision.clone(),
        source: TranscriptSource::AssemblyAi,
        provider: Some("assemblyAI".into()),
        source_payload_digest: ContentDigest::from_bytes([0x72; 32]),
        language: "en-US".into(),
        generated_at: UnixTimestampMilliseconds::new(1_900_000_003_000),
        speakers: Vec::new(),
        segments: vec![TranscriptArtifactSegmentInput {
            text: "A durable recovery observation.".into(),
            start_milliseconds: 0,
            end_milliseconds: 1_000,
            speaker_id: None,
            words: Vec::new(),
        }],
    }
}

fn workflow(fixture: &PlaybackFixture) -> pod0_application::TranscriptWorkflowProjection {
    workflow_for(&fixture.facade, fixture.episode_id)
}

fn workflow_stage(fixture: &PlaybackFixture) -> TranscriptWorkflowStage {
    workflow(fixture).stage
}

fn workflow_for(
    facade: &Pod0Facade,
    episode_id: EpisodeId,
) -> pod0_application::TranscriptWorkflowProjection {
    let Projection::TranscriptWorkflows { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::TranscriptWorkflows {
                episode_id: Some(episode_id),
            },
            offset: 0,
            max_items: 1,
        })
        .projection
    else {
        panic!("expected workflows");
    };
    value.workflows.into_iter().next().expect("workflow")
}

struct FixedClock(i64);

impl Clock for FixedClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0)
    }
}
