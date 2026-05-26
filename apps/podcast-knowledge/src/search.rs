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
#[path = "search_tests.rs"]
mod tests;
