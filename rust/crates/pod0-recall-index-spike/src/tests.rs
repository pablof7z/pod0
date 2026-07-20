use std::path::PathBuf;

use pod0_application::{
    EvidenceCandidateObservation, RecallEmbeddingVector, RecallScope, rank_evidence,
};
use pod0_domain::{EpisodeId, EvidenceGenerationId, EvidenceSpanId, PodcastId};
use serde::Deserialize;
use tempfile::tempdir;

use crate::{RecallCancellation, RecallIndexError, RecallIndexSpan, RecallIndexSpike};

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
fn shared_fixture_matches_typed_swift_candidate_contract() {
    let fixture = fixture();
    assert_eq!(fixture.fixture_version, 1);
    let mut index = RecallIndexSpike::in_memory(fixture.dimensions).unwrap();
    let cancellation = index.cancellation();
    for episode_low in [1, 2] {
        let spans = fixture_spans(&fixture, episode_low);
        assert_eq!(
            index.rebuild_episode(&spans, &cancellation).unwrap() as usize,
            spans.len()
        );
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
            &cancellation,
        )
        .unwrap();
    assert_eq!(candidates.len(), fixture.expected.len());
    for (candidate, expected) in candidates.iter().zip(&fixture.expected) {
        assert_eq!(
            candidate.span_id,
            EvidenceSpanId::from_parts(100, expected.span_low)
        );
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
fn disposable_index_rebuilds_after_deletion_from_only_selected_evidence() {
    let fixture = fixture();
    let directory = tempdir().unwrap();
    let path = directory.path().join("recall.sqlite");
    let spans = fixture_spans(&fixture, 1);
    {
        let mut index = RecallIndexSpike::open(&path, fixture.dimensions).unwrap();
        index
            .rebuild_episode(&spans, &index.cancellation())
            .unwrap();
        assert_eq!(index.stored_span_count().unwrap(), 3);
    }
    std::fs::remove_file(&path).unwrap();
    let mut rebuilt = RecallIndexSpike::open(&path, fixture.dimensions).unwrap();
    rebuilt
        .rebuild_episode(&spans, &rebuilt.cancellation())
        .unwrap();
    assert_eq!(rebuilt.stored_span_count().unwrap(), 3);
}

#[test]
fn cancellation_is_typed_and_never_commits_a_partial_generation() {
    let fixture = fixture();
    let mut index = RecallIndexSpike::in_memory(fixture.dimensions).unwrap();
    let cancellation = index.cancellation();
    cancellation.cancel();
    let result = index.rebuild_episode(&fixture_spans(&fixture, 1), &cancellation);
    assert!(matches!(result, Err(RecallIndexError::Cancelled)));
    assert_eq!(index.stored_span_count().unwrap(), 0);
}

#[test]
fn errors_and_debug_output_do_not_leak_private_transcript_text() {
    let private_text = "private transcript sentence must not enter diagnostics";
    let mut spans = fixture_spans(&fixture(), 1);
    spans[0].text = private_text.to_owned();
    spans[0].embedding.values.pop();
    let mut index = RecallIndexSpike::in_memory(4).unwrap();
    let error = index
        .rebuild_episode(&spans, &RecallCancellation::default())
        .unwrap_err();
    let diagnostic = format!("{error:?} {error}");
    assert!(!diagnostic.contains(private_text));
}

#[test]
fn prototype_reports_the_pinned_extension_version() {
    let index = RecallIndexSpike::in_memory(4).unwrap();
    assert_eq!(index.sqlite_vec_version().unwrap(), "v0.1.9");
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
            embedding: RecallEmbeddingVector {
                values: span.embedding.clone(),
            },
        })
        .collect()
}
