//! Cosine similarity + top-K search over a [`KnowledgeStore`].
//!
//! The implementation is intentionally trivial: no ANN index, no SIMD, no
//! external BLAS. A linear scan is fine for the in-memory baseline and
//! makes the math easy to audit. Hybrid (vector + BM25) ranking lands in
//! M6.B; this module exposes the raw KNN primitive.

use crate::store::KnowledgeStore;
use crate::types::SearchResult;

/// Compute cosine similarity between two equal-length vectors.
///
/// Returns `0.0` when either vector has zero magnitude, or when the
/// dimensions differ (we don't panic on mismatch — search callers may
/// feed mixed-provider embeddings and we'd rather return a deterministic
/// "no match" than crash the kernel).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0_f32;
    let mut mag_a = 0.0_f32;
    let mut mag_b = 0.0_f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        mag_a += a[i] * a[i];
        mag_b += b[i] * b[i];
    }
    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }
    dot / (mag_a.sqrt() * mag_b.sqrt())
}

/// Return the top-`k` chunks from `store` by cosine similarity against
/// `query_embedding`. Chunks without an embedding are skipped.
///
/// Results are sorted in descending score order. When the store has
/// fewer than `k` embedded chunks the returned vector is shorter than
/// `k`. When `k == 0` the result is always empty.
pub fn top_k_search(
    store: &KnowledgeStore,
    query_embedding: &[f32],
    k: usize,
) -> Vec<SearchResult> {
    if k == 0 || query_embedding.is_empty() {
        return Vec::new();
    }

    let mut scored: Vec<SearchResult> = store
        .embedded()
        .map(|(kc, emb)| SearchResult {
            chunk: kc.chunk.clone(),
            score: cosine_similarity(emb.as_slice(), query_embedding),
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(k);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EmbeddingVector, KnowledgeChunk, TranscriptChunk};

    fn mk(text: &str, idx: u32, embedding: Vec<f32>) -> KnowledgeChunk {
        KnowledgeChunk {
            chunk: TranscriptChunk {
                episode_id: "ep-1".into(),
                chunk_index: idx,
                start_secs: idx as f64,
                end_secs: (idx + 1) as f64,
                text: text.into(),
                word_count: text.split_whitespace().count() as u32,
            },
            embedding: Some(EmbeddingVector::new(embedding)),
        }
    }

    #[test]
    fn cosine_known_values() {
        // Identical unit vectors → 1.0.
        assert!((cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        // Orthogonal → 0.0.
        assert!((cosine_similarity(&[1.0, 0.0], &[0.0, 1.0])).abs() < 1e-6);
        // Anti-parallel → -1.0.
        assert!((cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6);
        // 3D unit vectors with known angle: a·b / (|a||b|) where
        // a=[1,2,3], b=[4,5,6] → 32 / (sqrt(14)*sqrt(77)) ≈ 0.97463.
        let s = cosine_similarity(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]);
        assert!((s - 0.974_631_8).abs() < 1e-5, "got {s}");
    }

    #[test]
    fn cosine_handles_zero_and_mismatched() {
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[1.0, 0.0, 0.0]), 0.0);
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn top_k_returns_ranked_subset() {
        let mut store = KnowledgeStore::new();
        store.upsert_many([
            mk("a", 0, vec![1.0, 0.0]),
            mk("b", 1, vec![0.0, 1.0]),
            mk("c", 2, vec![0.9, 0.1]),
            mk("d", 3, vec![-1.0, 0.0]),
            mk("e", 4, vec![0.5, 0.5]),
        ]);

        let results = top_k_search(&store, &[1.0, 0.0], 2);
        assert_eq!(results.len(), 2);
        // Best match should be the identical vector (text "a"), second
        // best the 0.9/0.1 lean (text "c").
        assert_eq!(results[0].chunk.text, "a");
        assert!((results[0].score - 1.0).abs() < 1e-6);
        assert_eq!(results[1].chunk.text, "c");
        // Scores are descending.
        assert!(results.windows(2).all(|w| w[0].score >= w[1].score));

        // Asking for a larger k returns every embedded chunk ranked.
        let all = top_k_search(&store, &[1.0, 0.0], 10);
        assert_eq!(all.len(), 5);
        assert!(all.windows(2).all(|w| w[0].score >= w[1].score));
        // The anti-parallel vector ("d") must be the lowest-ranked.
        assert_eq!(all.last().unwrap().chunk.text, "d");
    }

    #[test]
    fn top_k_skips_chunks_without_embedding() {
        let mut store = KnowledgeStore::new();
        store.upsert(KnowledgeChunk::without_embedding(TranscriptChunk {
            episode_id: "ep-1".into(),
            chunk_index: 0,
            start_secs: 0.0,
            end_secs: 1.0,
            text: "no-vector".into(),
            word_count: 1,
        }));
        store.upsert(mk("with-vector", 1, vec![1.0, 0.0]));

        let results = top_k_search(&store, &[1.0, 0.0], 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk.text, "with-vector");
    }

    #[test]
    fn top_k_zero_returns_empty() {
        let mut store = KnowledgeStore::new();
        store.upsert(mk("a", 0, vec![1.0, 0.0]));
        assert!(top_k_search(&store, &[1.0, 0.0], 0).is_empty());
    }
}
