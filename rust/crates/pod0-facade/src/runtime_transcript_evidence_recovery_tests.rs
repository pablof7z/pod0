use pod0_application::{
    ApplicationCommand, CommandEnvelope, DurableRecallHostObservation, HostObservation,
    HostObservationEnvelope, HostRequest, LeasedHostObservationEnvelope, RecallEmbeddingVector,
    RecallSpanEmbeddingObservation, TranscriptCapabilityObservation, TranscriptCapabilityRequest,
    TranscriptWorkflowOrigin, TranscriptWorkflowStage,
};
use pod0_domain::{CancellationId, CommandId, UnixTimestampMilliseconds};
use pod0_storage::{EvidenceObservationCommitInput, LibraryStore};

use crate::Pod0Facade;
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::runtime_transcript_workflow_tests::{configuration, transcript, workflow};

#[test]
fn durable_recall_observation_rebuilds_the_index_after_process_loss() {
    let fixture = PlaybackFixture::new();
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(75, 1),
        cancellation_id: CancellationId::from_parts(75, 2),
        expected_revision: None,
        command: ApplicationCommand::EnsureTranscriptWorkflow {
            episode_id: fixture.episode_id,
            origin: TranscriptWorkflowOrigin::User,
            configuration: configuration(),
        },
    });
    let transcript_lease = fixture
        .facade
        .next_leased_host_requests(1)
        .pop()
        .expect("transcript lease");
    let HostRequest::ExecuteTranscriptCapability {
        capability: TranscriptCapabilityRequest::SubmitProvider { context, .. },
    } = &transcript_lease.request.request
    else {
        panic!("provider request")
    };
    fixture
        .facade
        .record_leased_host_observation(LeasedHostObservationEnvelope {
            lease: transcript_lease.lease,
            observation: HostObservationEnvelope {
                request_id: transcript_lease.request.request_id,
                cancellation_id: transcript_lease.request.cancellation_id,
                observed_request_revision: transcript_lease.request.issued_revision,
                sequence_number: 0,
                observed_at: UnixTimestampMilliseconds::new(
                    transcript_lease.lease.expires_at.value - 1,
                ),
                observation: HostObservation::TranscriptCapabilityObserved {
                    observation: TranscriptCapabilityObservation::Completed {
                        external_operation_id: None,
                        provider_status: Some("completed".into()),
                        artifact: transcript(context),
                    },
                },
            },
        });
    let recall_lease = fixture
        .facade
        .next_leased_host_requests(1)
        .pop()
        .expect("recall lease");
    let HostRequest::EmbedRecallSpans {
        episode_id,
        generation_id,
        spans,
        ..
    } = &recall_lease.request.request
    else {
        panic!("recall request")
    };
    let observation = HostObservationEnvelope {
        request_id: recall_lease.request.request_id,
        cancellation_id: recall_lease.request.cancellation_id,
        observed_request_revision: recall_lease.request.issued_revision,
        sequence_number: 0,
        observed_at: UnixTimestampMilliseconds::new(recall_lease.lease.expires_at.value - 1),
        observation: HostObservation::RecallSpansEmbedded {
            episode_id: *episode_id,
            generation_id: *generation_id,
            embeddings: spans
                .iter()
                .map(|span| RecallSpanEmbeddingObservation {
                    span_id: span.span_id,
                    embedding: RecallEmbeddingVector {
                        values: crate::runtime_recall_test_support::recall_test_embedding(),
                    },
                })
                .collect(),
        },
    };
    LibraryStore::open_authoritative(&fixture.target)
        .unwrap()
        .commit_evidence_observation(EvidenceObservationCommitInput {
            lease: recall_lease.lease,
            observation: DurableRecallHostObservation::from_host(&observation).unwrap(),
            committed_at: observation.observed_at,
        })
        .unwrap();
    assert_eq!(
        workflow(&fixture.facade, fixture.episode_id).stage,
        TranscriptWorkflowStage::EvidenceRequested
    );

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let _ = reopened.next_leased_host_requests(1);
    assert_eq!(
        workflow(&reopened, fixture.episode_id).stage,
        TranscriptWorkflowStage::Succeeded
    );
}
