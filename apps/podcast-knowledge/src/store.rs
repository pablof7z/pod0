//! In-memory knowledge store.
//!
//! Holds a flat `Vec<KnowledgeChunk>` keyed implicitly by
//! `(episode_id, chunk_index)`. The persistent LMDB-backed store lands in
//! M6.B alongside `nmp.vector.capability`; this baseline exists so the
//! upstream pipeline (transcript ingest → chunking → embedding) has a
//! sink to write to today and the kernel projection layer has something
//! to read from.
//!
//! Upserts are idempotent: re-ingesting the same chunk replaces the prior
//! entry rather than accumulating duplicates.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::{EmbeddingVector, KnowledgeChunk};

/// In-memory chunk store.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeStore {
    pub chunks: Vec<KnowledgeChunk>,
}

impl KnowledgeStore {
    /// Empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of chunks currently stored.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// True when the store contains no chunks.
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Insert (or replace) a single chunk. Idempotent on
    /// `(episode_id, chunk_index)`.
    pub fn upsert(&mut self, chunk: KnowledgeChunk) {
        let key = (chunk.chunk.episode_id.clone(), chunk.chunk.chunk_index);
        if let Some(slot) = self.chunks.iter_mut().find(|c| {
            c.chunk.episode_id == key.0 && c.chunk.chunk_index == key.1
        }) {
            *slot = chunk;
        } else {
            self.chunks.push(chunk);
        }
    }

    /// Bulk upsert. Re-uses [`upsert`] for the per-chunk key replacement.
    pub fn upsert_many(&mut self, chunks: impl IntoIterator<Item = KnowledgeChunk>) {
        for c in chunks {
            self.upsert(c);
        }
    }

    /// Remove every chunk belonging to `episode_id`. Used when an episode
    /// is re-ingested with a new transcript and we want a clean slate.
    pub fn delete_episode(&mut self, episode_id: &str) -> usize {
        let before = self.chunks.len();
        self.chunks.retain(|c| c.chunk.episode_id != episode_id);
        before - self.chunks.len()
    }

    /// Iterator over chunks that have an embedding populated.
    pub fn embedded(&self) -> impl Iterator<Item = (&KnowledgeChunk, &EmbeddingVector)> {
        self.chunks
            .iter()
            .filter_map(|c| c.embedding.as_ref().map(|e| (c, e)))
    }

    /// Build a lookup keyed by `(episode_id, chunk_index)` for callers
    /// that need O(1) access while iterating. The map borrows the store
    /// so it's cheap to construct.
    pub fn index_map(&self) -> HashMap<(&str, u32), &KnowledgeChunk> {
        self.chunks
            .iter()
            .map(|c| ((c.chunk.episode_id.as_str(), c.chunk.chunk_index), c))
            .collect()
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
