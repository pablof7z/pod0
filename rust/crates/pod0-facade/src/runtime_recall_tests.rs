use crate::runtime_playback_test_support::PlaybackFixture;
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

    fixture.dispatch(23, 23, "question without a prepared transcript");
    assert_eq!(fixture.projection(23).stage, RecallStage::TranscriptMissing);
    assert!(fixture.base.facade.next_host_requests(u16::MAX).is_empty());

    let transcript_without_index = PlaybackFixture::new_with_transcript(true);
    transcript_without_index.facade.dispatch(recall_command(
        26,
        26,
        "question without selected evidence",
        RecallScope::Episode {
            episode_id: transcript_without_index.episode_id,
        },
        2,
    ));
    assert_eq!(
        recall_projection(&transcript_without_index.facade, 26).stage,
        RecallStage::IndexMissing
    );

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
        RecallStage::ProviderUnavailable
    );
}

#[test]
fn indexing_empty_results_and_process_restart_are_explicit() {
    let indexing = RecallFixture::new(false);
    indexing.base.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(40, 1),
        cancellation_id: CancellationId::from_parts(41, 1),
        expected_revision: None,
        command: ApplicationCommand::RebuildTranscriptEvidence {
            input: evidence_input(&indexing.base),
            policy: evidence_policy(),
        },
    });
    indexing.dispatch(41, 41, "question during indexing");
    assert_eq!(indexing.projection(41).stage, RecallStage::Indexing);

    let empty = RecallFixture::new(true);
    empty.dispatch(42, 42, "evidence-free result");
    let embed = empty.base.facade.next_host_requests(1).pop().unwrap();
    record(
        &empty.base.facade,
        &embed,
        HostObservation::RecallQueryEmbedded {
            query_id: RecallQueryId::from_parts(32, 42),
            embedding: RecallEmbeddingVector { values: vec![1] },
        },
    );
    let retrieve = empty.base.facade.next_host_requests(1).pop().unwrap();
    record(
        &empty.base.facade,
        &retrieve,
        HostObservation::RecallCandidatesRetrieved {
            query_id: RecallQueryId::from_parts(32, 42),
            candidates: Vec::new(),
        },
    );
    assert_eq!(empty.projection(42).stage, RecallStage::NoEvidence);

    let interrupted = RecallFixture::new(true);
    interrupted.dispatch(43, 43, "interrupted query");
    let reopened =
        Pod0Facade::open(interrupted.base.target.to_string_lossy().into_owned()).unwrap();
    assert_eq!(
        recall_projection(&reopened, 43).stage,
        RecallStage::Interrupted
    );
    reopened.dispatch(recall_command(
        44,
        43,
        "interrupted query",
        RecallScope::Episode {
            episode_id: interrupted.base.episode_id,
        },
        2,
    ));
    assert!(matches!(
        recall_projection(&reopened, 43).stage,
        RecallStage::Queued | RecallStage::Running { .. }
    ));
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
    assert_eq!(malformed.projection(30).stage, RecallStage::CorruptArtifact);

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
