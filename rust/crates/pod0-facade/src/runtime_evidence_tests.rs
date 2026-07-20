use pod0_application::TranscriptSegmentInput;

use crate::runtime_recall_test_support::RecallFixture;
use crate::*;

#[test]
fn rebuild_selects_exact_generation_and_exposes_bounded_pages() {
    let fixture = RecallFixture::new(false);
    let input = input_from_fixture(&fixture);
    let envelope = rebuild_command(1, &fixture, input);

    fixture.base.facade.dispatch(envelope.clone());

    let request = fixture.base.facade.next_host_requests(1).pop().unwrap();
    assert!(matches!(
        &request.request,
        HostRequest::EmbedRecallSpans {
            episode_id,
            generation_id,
            spans,
            ..
        } if *episode_id == fixture.base.episode_id
            && *generation_id == fixture.artifact.generation_id
            && spans.len() == fixture.artifact.spans.len()
    ));
    let first = evidence_page(&fixture.base.facade, fixture.base.episode_id, 0, 1);
    assert_eq!(first.stage, EvidenceIndexStage::Ready);
    assert_eq!(first.generation_id, Some(fixture.artifact.generation_id));
    assert_eq!(first.spans.len(), 1);
    assert!(first.has_more);
    assert_eq!(first.spans[0].span_id, fixture.artifact.spans[0].span_id);
    assert_eq!(first.spans[0].text, fixture.artifact.spans[0].text);

    record_rebuild_success(&fixture.base.facade, &request);
    let operation = operation(&fixture.base.facade, envelope.command_id);
    assert_eq!(operation.stage, OperationStage::Succeeded);
    assert!(matches!(
        operation.result,
        Some(OperationResult::EvidenceRebuilt {
            episode_id,
            generation_id,
            span_count,
        }) if episode_id == fixture.base.episode_id
            && generation_id == fixture.artifact.generation_id
            && span_count == first.total_spans
    ));
}

#[test]
fn interrupted_rebuild_restarts_idempotently_after_facade_reopen() {
    let fixture = RecallFixture::new(false);
    let input = input_from_fixture(&fixture);
    fixture
        .base
        .facade
        .dispatch(rebuild_command(2, &fixture, input.clone()));
    let abandoned = fixture.base.facade.next_host_requests(1).pop().unwrap();

    let reopened = Pod0Facade::open(fixture.base.target.to_string_lossy().into_owned()).unwrap();
    let restarted = rebuild_command(3, &fixture, input);
    reopened.dispatch(restarted.clone());
    let request = reopened.next_host_requests(1).pop().unwrap();
    assert_ne!(request.request_id, abandoned.request_id);
    assert_eq!(request.request, abandoned.request);
    let page = evidence_page(&reopened, fixture.base.episode_id, 0, u16::MAX);
    assert_eq!(page.generation_id, Some(fixture.artifact.generation_id));

    record_rebuild_success(&reopened, &request);
    assert_eq!(
        operation(&reopened, restarted.command_id).stage,
        OperationStage::Succeeded
    );
}

#[test]
fn cancellation_and_malformed_rebuild_observations_fail_closed() {
    let cancelled = RecallFixture::new(false);
    let command = rebuild_command(4, &cancelled, input_from_fixture(&cancelled));
    cancelled.base.facade.dispatch(command.clone());
    let request = cancelled.base.facade.next_host_requests(1).pop().unwrap();
    cancelled.base.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(40, 5),
        cancellation_id: CancellationId::from_parts(41, 5),
        expected_revision: None,
        command: ApplicationCommand::CancelOperation {
            cancellation_id: command.cancellation_id,
        },
    });
    let revision = cancelled
        .base
        .facade
        .snapshot(library_request())
        .state_revision;
    record_rebuild_success(&cancelled.base.facade, &request);
    assert_eq!(
        cancelled
            .base
            .facade
            .snapshot(library_request())
            .state_revision,
        revision
    );
    assert_eq!(
        operation(&cancelled.base.facade, command.command_id).stage,
        OperationStage::Cancelled
    );

    let malformed = RecallFixture::new(false);
    let command = rebuild_command(6, &malformed, input_from_fixture(&malformed));
    malformed.base.facade.dispatch(command.clone());
    let request = malformed.base.facade.next_host_requests(1).pop().unwrap();
    record_rebuild_malformed(&malformed.base.facade, &request);
    let failed = operation(&malformed.base.facade, command.command_id);
    assert_eq!(failed.stage, OperationStage::Failed);
    assert_eq!(
        failed.failure.map(|failure| failure.code),
        Some(CoreFailureCode::HostRejected)
    );
}

fn input_from_fixture(fixture: &RecallFixture) -> TranscriptEvidenceInput {
    TranscriptEvidenceInput {
        episode_id: fixture.artifact.version.episode_id,
        podcast_id: fixture.artifact.version.podcast_id,
        source_revision: fixture.artifact.version.source_revision.clone(),
        source: fixture.artifact.version.provenance.source,
        provider: fixture.artifact.version.provenance.provider.clone(),
        source_payload_digest: fixture.artifact.version.provenance.source_payload_digest,
        segments: fixture
            .artifact
            .segments
            .iter()
            .map(|segment| TranscriptSegmentInput {
                text: segment.text.clone(),
                start_milliseconds: segment.start_milliseconds,
                end_milliseconds: segment.end_milliseconds,
                speaker_id: segment.speaker_id,
            })
            .collect(),
    }
}

fn rebuild_command(
    id: u64,
    fixture: &RecallFixture,
    input: TranscriptEvidenceInput,
) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(40, id),
        cancellation_id: CancellationId::from_parts(41, id),
        expected_revision: None,
        command: ApplicationCommand::RebuildTranscriptEvidence {
            input,
            policy: fixture.artifact.policy,
        },
    }
}

fn evidence_page(
    facade: &Pod0Facade,
    episode_id: EpisodeId,
    offset: u32,
    max_items: u16,
) -> EvidenceIndexProjection {
    let Projection::EvidenceIndex { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::EvidenceIndex { episode_id },
            offset,
            max_items,
        })
        .projection
    else {
        panic!("expected evidence-index projection");
    };
    value
}

fn record_rebuild_success(facade: &Pod0Facade, request: &HostRequestEnvelope) {
    let HostRequest::EmbedRecallSpans {
        episode_id,
        generation_id,
        spans,
        ..
    } = &request.request
    else {
        panic!("expected embedding request");
    };
    facade.record_host_observation(HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: 0,
        observed_at: UnixTimestampMilliseconds::new(1_800_000_000_300),
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
    });
}

fn record_rebuild_malformed(facade: &Pod0Facade, request: &HostRequestEnvelope) {
    let HostRequest::EmbedRecallSpans {
        episode_id,
        generation_id,
        spans,
        ..
    } = &request.request
    else {
        panic!("expected embedding request");
    };
    facade.record_host_observation(HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: 0,
        observed_at: UnixTimestampMilliseconds::new(1_800_000_000_300),
        observation: HostObservation::RecallSpansEmbedded {
            episode_id: *episode_id,
            generation_id: *generation_id,
            embeddings: spans
                .iter()
                .map(|span| RecallSpanEmbeddingObservation {
                    span_id: span.span_id,
                    embedding: RecallEmbeddingVector { values: vec![1] },
                })
                .collect(),
        },
    });
}

fn library_request() -> ProjectionRequest {
    ProjectionRequest {
        scope: ProjectionScope::Library,
        offset: 0,
        max_items: 20,
    }
}

fn operation(facade: &Pod0Facade, command_id: CommandId) -> OperationProjection {
    let Projection::Library { value } = facade.snapshot(library_request()).projection else {
        panic!("expected library projection");
    };
    value
        .operations
        .into_iter()
        .find(|operation| operation.command_id == command_id)
        .expect("operation should be projected")
}
