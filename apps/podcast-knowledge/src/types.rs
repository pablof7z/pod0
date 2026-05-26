//! Knowledge-layer types.
//!
//! A [`KnowledgeChunk`] is a [`TranscriptChunk`] paired with an optional
//! embedding vector. The vector is populated by the STT provider (or a
//! follow-up embedding call) and consumed by [`crate::search`] for
//! semantic search.
//!
//! [`SearchResult`] carries the cosine similarity score alongside the
//! matched chunk so callers can render ranked output.

use serde::{Deserialize, Serialize};

pub use podcast_transcripts::TranscriptChunk;

/// Embedding vector. Wraps a `Vec<f32>` so the type is named at the API
/// boundary, but is `transparent` for serialisation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EmbeddingVector(pub Vec<f32>);

impl EmbeddingVector {
    /// Construct an embedding vector from a `Vec<f32>`.
    pub fn new(values: Vec<f32>) -> Self {
        Self(values)
    }

    /// Dimensionality of the vector.
    pub fn dim(&self) -> usize {
        self.0.len()
    }

    /// Read-only access to the underlying values.
    pub fn as_slice(&self) -> &[f32] {
        &self.0
    }
}

impl From<Vec<f32>> for EmbeddingVector {
    fn from(values: Vec<f32>) -> Self {
        Self(values)
    }
}

/// One transcript chunk plus its (optional) embedding.
///
/// Embeddings arrive either alongside the STT output (ElevenLabs Scribe,
/// AssemblyAI) or via a follow-up call to an embedding model. Chunks
/// without an embedding still live in the store — they're returned by
/// keyword search but skipped by [`crate::search::top_k_search`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeChunk {
    pub chunk: TranscriptChunk,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<EmbeddingVector>,
}

impl KnowledgeChunk {
    /// Construct a chunk without an embedding (lexical-only).
    pub fn without_embedding(chunk: TranscriptChunk) -> Self {
        Self {
            chunk,
            embedding: None,
        }
    }

    /// Construct a chunk with a provided embedding.
    pub fn with_embedding(chunk: TranscriptChunk, embedding: impl Into<EmbeddingVector>) -> Self {
        Self {
            chunk,
            embedding: Some(embedding.into()),
        }
    }
}

/// One ranked search hit. `score` is cosine similarity in `[-1.0, 1.0]`
/// where 1.0 is a perfect match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub chunk: TranscriptChunk,
    pub score: f32,
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
