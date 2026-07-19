use crate::runtime_recall_test_support::*;
use crate::*;

#[test]
fn recall_is_deterministic_bounded_and_preserves_exact_playable_evidence() {
    let first = run_ready_recall(false);
    let second = run_ready_recall(false);
    assert_eq!(first, second);
    assert_eq!(first.stage, RecallStage::Ready);
    assert_eq!(first.evidence.len(), 2);
    assert!(first.evidence[0].score.rerank_rank < first.evidence[1].score.rerank_rank);
    let evidence = &first.evidence[0];
    assert_eq!(evidence.episode_id, EpisodeId::from_bytes([0x22; 16]));
    assert!(evidence.end_milliseconds > evidence.start_milliseconds);
    assert!(evidence.end_segment_ordinal_exclusive > evidence.start_segment_ordinal);
    assert!(!evidence.excerpt.is_empty());
    assert!(evidence.excerpt.len() <= MAX_RECALL_EXCERPT_BYTES);
    assert_eq!(evidence.provenance.source, TranscriptSource::Publisher);
    assert_eq!(
        evidence.provenance.provider.as_deref(),
        Some("fixture-provider")
    );
    assert!(evidence.score.total_rrf_units > 0);
    assert!(matches!(
        first.operation.and_then(|operation| operation.result),
        Some(OperationResult::RecallFinished {
            evidence_count: 2,
            ..
        })
    ));
}

#[test]
fn cancellation_rejects_late_and_duplicate_observations() {
    let fixture = RecallFixture::new(true);
    let envelope = fixture.dispatch(10, 10, "durable habits");
    let embedding = fixture.base.facade.next_host_requests(1).pop().unwrap();
    assert_eq!(fixture.projection(10).stage, RecallStage::Queued);

    fixture.base.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(30, 11),
        cancellation_id: CancellationId::from_parts(31, 11),
        expected_revision: None,
        command: ApplicationCommand::CancelOperation {
            cancellation_id: envelope.cancellation_id,
        },
    });
    let cancelled = fixture.projection(10);
    assert_eq!(cancelled.stage, RecallStage::Cancelled);
    let revision = fixture
        .base
        .facade
        .snapshot(recall_request(10))
        .state_revision;
    record(
        &fixture.base.facade,
        &embedding,
        HostObservation::RecallQueryEmbedded {
            query_id: RecallQueryId::from_parts(32, 10),
            embedding: RecallEmbeddingVector { values: vec![1] },
        },
    );
    assert_eq!(
        fixture
            .base
            .facade
            .snapshot(recall_request(10))
            .state_revision,
        revision
    );
    fixture.base.facade.dispatch(envelope);
    assert_eq!(fixture.projection(10), cancelled);
}

#[test]
fn invalid_missing_and_unsupported_queries_have_explicit_terminal_state() {
    let fixture = RecallFixture::new(false);
    fixture
        .base
        .facade
        .dispatch(recall_command(20, 20, "  \n ", RecallScope::Library, 2));
    assert_eq!(fixture.projection(20).stage, RecallStage::Failed);

    fixture.base.facade.dispatch(recall_command(
        21,
        21,
        &"x".repeat(MAX_RECALL_QUERY_BYTES + 1),
        RecallScope::Library,
        2,
    ));
    assert_eq!(fixture.projection(21).stage, RecallStage::Failed);

    fixture.base.facade.dispatch(recall_command(
        22,
        22,
        "question",
        RecallScope::Unsupported { wire_code: 77 },
        2,
    ));
    assert_eq!(
        fixture.projection(22).stage,
        RecallStage::Unsupported { wire_code: 77 }
    );

    fixture.dispatch(23, 23, "question without selected evidence");
    assert_eq!(fixture.projection(23).stage, RecallStage::NoEvidence);
    assert!(fixture.base.facade.next_host_requests(u16::MAX).is_empty());

    let unavailable = Pod0Facade::new();
    unavailable.dispatch(recall_command(24, 24, "question", RecallScope::Library, 2));
    assert_eq!(
        recall_projection(&unavailable, 24).stage,
        RecallStage::IndexUnavailable
    );

    let provider_failure = RecallFixture::new(true);
    provider_failure.dispatch(25, 25, "question with provider failure");
    let embed = provider_failure
        .base
        .facade
        .next_host_requests(1)
        .pop()
        .unwrap();
    record(
        &provider_failure.base.facade,
        &embed,
        HostObservation::Failed {
            code: HostFailureCode::ProviderUnavailable,
            safe_detail: None,
        },
    );
    assert_eq!(
        provider_failure.projection(25).stage,
        RecallStage::IndexUnavailable
    );
}

#[test]
fn malformed_candidates_fail_closed_and_optional_rerank_falls_back() {
    let malformed = RecallFixture::new(true);
    malformed.dispatch(30, 30, "habit cues");
    let embed = malformed.base.facade.next_host_requests(1).pop().unwrap();
    record(
        &malformed.base.facade,
        &embed,
        HostObservation::RecallQueryEmbedded {
            query_id: RecallQueryId::from_parts(32, 30),
            embedding: RecallEmbeddingVector {
                values: vec![10, -10],
            },
        },
    );
    let retrieve = malformed.base.facade.next_host_requests(1).pop().unwrap();
    record(
        &malformed.base.facade,
        &retrieve,
        HostObservation::RecallCandidatesRetrieved {
            query_id: RecallQueryId::from_parts(32, 30),
            candidates: vec![RecallCandidateObservation {
                episode_id: malformed.base.episode_id,
                generation_id: EvidenceGenerationId::from_parts(99, 99),
                span_id: malformed.artifact.spans[0].span_id,
                vector_rank: Some(1),
                lexical_rank: None,
            }],
        },
    );
    assert_eq!(malformed.projection(30).stage, RecallStage::Failed);

    let fallback = RecallFixture::new(true);
    fallback.dispatch(31, 31, "habit cues");
    advance_to_rerank(&fallback, 31);
    let rerank = fallback.base.facade.next_host_requests(1).pop().unwrap();
    record(
        &fallback.base.facade,
        &rerank,
        HostObservation::Failed {
            code: HostFailureCode::ProviderUnavailable,
            safe_detail: None,
        },
    );
    let projection = fallback.projection(31);
    assert_eq!(projection.stage, RecallStage::Ready);
    assert!(
        projection
            .evidence
            .iter()
            .all(|item| item.score.rerank_rank.is_none())
    );
}

fn run_ready_recall(rerank_failure: bool) -> RecallResultProjection {
    let fixture = RecallFixture::new(true);
    fixture.dispatch(1, 1, "  durable   habit cues ");
    advance_to_rerank(&fixture, 1);
    let rerank = fixture.base.facade.next_host_requests(1).pop().unwrap();
    let HostRequest::RerankRecallCandidates { candidates, .. } = &rerank.request else {
        panic!("expected rerank request");
    };
    let revision = fixture
        .base
        .facade
        .snapshot(recall_request(1))
        .state_revision;
    let observation = if rerank_failure {
        HostObservation::Failed {
            code: HostFailureCode::ProviderUnavailable,
            safe_detail: None,
        }
    } else {
        HostObservation::RecallCandidatesReranked {
            query_id: RecallQueryId::from_parts(32, 1),
            rankings: vec![
                RecallRerankObservation {
                    span_id: candidates[1].span_id,
                    rank: 1,
                },
                RecallRerankObservation {
                    span_id: candidates[0].span_id,
                    rank: 2,
                },
            ],
        }
    };
    record(&fixture.base.facade, &rerank, observation.clone());
    let projection = fixture.projection(1);
    record(&fixture.base.facade, &rerank, observation);
    assert_eq!(
        fixture
            .base
            .facade
            .snapshot(recall_request(1))
            .state_revision,
        StateRevision::new(revision.value + 1)
    );
    projection
}

fn advance_to_rerank(fixture: &RecallFixture, query_id: u64) {
    let embed = fixture.base.facade.next_host_requests(1).pop().unwrap();
    let HostRequest::EmbedRecallQuery { text, .. } = &embed.request else {
        panic!("expected embedding request");
    };
    assert!(!text.contains("  "));
    record(
        &fixture.base.facade,
        &embed,
        HostObservation::RecallQueryEmbedded {
            query_id: RecallQueryId::from_parts(32, query_id),
            embedding: RecallEmbeddingVector {
                values: vec![100, -200, 300],
            },
        },
    );
    assert_eq!(
        fixture.projection(query_id).stage,
        RecallStage::Running {
            phase: RecallPhase::Retrieving
        }
    );
    let retrieve = fixture.base.facade.next_host_requests(1).pop().unwrap();
    let candidates = fixture
        .artifact
        .spans
        .iter()
        .take(2)
        .enumerate()
        .map(|(index, span)| RecallCandidateObservation {
            episode_id: fixture.base.episode_id,
            generation_id: fixture.artifact.generation_id,
            span_id: span.span_id,
            vector_rank: Some(u16::try_from(index + 1).unwrap()),
            lexical_rank: Some(u16::try_from(2 - index).unwrap()),
        })
        .collect();
    record(
        &fixture.base.facade,
        &retrieve,
        HostObservation::RecallCandidatesRetrieved {
            query_id: RecallQueryId::from_parts(32, query_id),
            candidates,
        },
    );
    assert_eq!(
        fixture.projection(query_id).stage,
        RecallStage::Running {
            phase: RecallPhase::Reranking
        }
    );
}
