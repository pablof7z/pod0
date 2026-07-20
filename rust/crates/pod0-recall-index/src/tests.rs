use std::path::PathBuf;

use pod0_application::{
    EvidenceCandidateObservation, RecallEmbeddingVector, RecallScope, rank_evidence,
};
use pod0_domain::{EpisodeId, EvidenceGenerationId, EvidenceSpanId, PodcastId};
use serde::Deserialize;
use tempfile::tempdir;

use crate::{
    RecallCancellation, RecallIndex, RecallIndexError, RecallIndexPlan, RecallIndexSpan,
    RecallSpanEmbedding,
};

#[derive(Deserialize)]
struct Fixture {
    fixture_version: u32,
    dimensions: usize,
    lexical_query: String,
    query_embedding: Vec<i32>,
    query_episode_low: u64,
    spans: Vec<FixtureSpan>,
    expected: Vec<ExpectedCandidate>,
    expected_ranked_span_lows: Vec<u64>,
}

#[derive(Deserialize)]
struct FixtureSpan {
    span_low: u64,
    generation_low: u64,
    episode_low: u64,
    podcast_low: u64,
    text: String,
    embedding: Vec<i32>,
}

#[derive(Deserialize)]
struct ExpectedCandidate {
    span_low: u64,
    vector_rank: Option<u16>,
    lexical_rank: Option<u16>,
}

#[test]
fn shared_fixture_preserves_raw_lanes_and_rust_ranking() {
    let fixture = fixture();
    assert_eq!(fixture.fixture_version, 1);
    let mut index = RecallIndex::in_memory(fixture.dimensions).unwrap();
    for episode_low in [1, 2] {
        index_fixture_episode(&mut index, &fixture, episode_low);
    }
    let candidates = index
        .retrieve(
            &RecallEmbeddingVector {
                values: fixture.query_embedding,
            },
            &fixture.lexical_query,
            RecallScope::Episode {
                episode_id: EpisodeId::from_parts(300, fixture.query_episode_low),
            },
            3,
            3,
            6,
            &index.cancellation(),
        )
        .unwrap();
    assert_eq!(candidates.len(), fixture.expected.len());
    for (candidate, expected) in candidates.iter().zip(&fixture.expected) {
        assert_eq!(candidate.span_id.low, expected.span_low);
        assert_eq!(candidate.vector_rank, expected.vector_rank);
        assert_eq!(candidate.lexical_rank, expected.lexical_rank);
    }
    let ranked = rank_evidence(
        &candidates
            .iter()
            .map(|candidate| EvidenceCandidateObservation {
                span_id: candidate.span_id,
                vector_rank: candidate.vector_rank,
                lexical_rank: candidate.lexical_rank,
            })
            .collect::<Vec<_>>(),
        3,
    )
    .unwrap();
    assert_eq!(
        ranked
            .iter()
            .map(|candidate| candidate.span_id.low)
            .collect::<Vec<_>>(),
        fixture.expected_ranked_span_lows
    );
}

#[test]
fn cached_embeddings_recover_after_restart_without_provider_work() {
    let fixture = fixture();
    let directory = tempdir().unwrap();
    let path = directory.path().join("recall.sqlite");
    let spans = fixture_spans(&fixture, 1);
    {
        let mut index = RecallIndex::open(&path, fixture.dimensions).unwrap();
        let first = requested(&mut index, &spans);
        assert_eq!(first.len(), spans.len());
        index
            .cache_embeddings(
                &spans[..1],
                &fixture_embeddings(&fixture, &first[..1]),
                &index.cancellation(),
            )
            .unwrap();
    }
    let mut reopened = RecallIndex::open(&path, fixture.dimensions).unwrap();
    let remaining = requested(&mut reopened, &spans);
    assert_eq!(remaining.len(), spans.len() - 1);
    reopened
        .cache_embeddings(
            &spans[1..],
            &fixture_embeddings(&fixture, &remaining),
            &reopened.cancellation(),
        )
        .unwrap();
    assert!(matches!(
        reopened
            .prepare_episode(&spans, &reopened.cancellation())
            .unwrap(),
        RecallIndexPlan::Ready {
            indexed_span_count: 3
        }
    ));
    reopened.reset_execution_tables().unwrap();
    assert!(matches!(
        reopened
            .prepare_episode(&spans, &reopened.cancellation())
            .unwrap(),
        RecallIndexPlan::Ready {
            indexed_span_count: 3
        }
    ));
}

#[test]
fn cancellation_never_commits_a_partial_generation() {
    let fixture = fixture();
    let spans = fixture_spans(&fixture, 1);
    let mut index = RecallIndex::in_memory(fixture.dimensions).unwrap();
    let requests = requested(&mut index, &spans);
    index
        .cache_embeddings(
            &spans,
            &fixture_embeddings(&fixture, &requests),
            &index.cancellation(),
        )
        .unwrap();
    let cancellation = index.cancellation();
    cancellation.cancel();
    let result = index.prepare_episode(&spans, &cancellation);
    assert!(matches!(result, Err(RecallIndexError::Cancelled)));
    assert_eq!(index.stored_span_count().unwrap(), 0);
}

#[test]
fn diagnostics_never_include_private_text_and_extension_is_pinned() {
    let private_text = "private transcript sentence must not enter diagnostics";
    let mut spans = fixture_spans(&fixture(), 1);
    spans[1].span_id = spans[0].span_id;
    spans[0].text = private_text.to_owned();
    let mut index = RecallIndex::in_memory(4).unwrap();
    let error = index
        .prepare_episode(&spans, &RecallCancellation::default())
        .unwrap_err();
    let diagnostic = format!("{error:?} {error}");
    assert!(!diagnostic.contains(private_text));
    assert_eq!(index.sqlite_vec_version().unwrap(), "v0.1.9");
}

fn requested(index: &mut RecallIndex, spans: &[RecallIndexSpan]) -> Vec<EvidenceSpanId> {
    let RecallIndexPlan::NeedsEmbeddings { spans } =
        index.prepare_episode(spans, &index.cancellation()).unwrap()
    else {
        panic!("expected missing embeddings")
    };
    spans.into_iter().map(|span| span.span_id).collect()
}

fn index_fixture_episode(index: &mut RecallIndex, fixture: &Fixture, episode_low: u64) {
    let spans = fixture_spans(fixture, episode_low);
    loop {
        match index
            .prepare_episode(&spans, &index.cancellation())
            .unwrap()
        {
            RecallIndexPlan::Ready { .. } => break,
            RecallIndexPlan::NeedsEmbeddings { spans: requests } => {
                let ids = requests
                    .iter()
                    .map(|request| request.span_id)
                    .collect::<Vec<_>>();
                let batch = spans
                    .iter()
                    .filter(|span| ids.contains(&span.span_id))
                    .cloned()
                    .collect::<Vec<_>>();
                index
                    .cache_embeddings(
                        &batch,
                        &fixture_embeddings(fixture, &ids),
                        &index.cancellation(),
                    )
                    .unwrap();
            }
        }
    }
}

fn fixture() -> Fixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../Fixtures/CoreKnowledge/recall-index-v1.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn fixture_spans(fixture: &Fixture, episode_low: u64) -> Vec<RecallIndexSpan> {
    fixture
        .spans
        .iter()
        .filter(|span| span.episode_low == episode_low)
        .map(|span| RecallIndexSpan {
            span_id: EvidenceSpanId::from_parts(100, span.span_low),
            generation_id: EvidenceGenerationId::from_parts(200, span.generation_low),
            episode_id: EpisodeId::from_parts(300, span.episode_low),
            podcast_id: PodcastId::from_parts(400, span.podcast_low),
            text: span.text.clone(),
        })
        .collect()
}

fn fixture_embeddings(fixture: &Fixture, ids: &[EvidenceSpanId]) -> Vec<RecallSpanEmbedding> {
    ids.iter()
        .map(|id| {
            let span = fixture
                .spans
                .iter()
                .find(|span| span.span_low == id.low)
                .unwrap();
            RecallSpanEmbedding {
                span_id: *id,
                embedding: RecallEmbeddingVector {
                    values: span.embedding.clone(),
                },
            }
        })
        .collect()
}
