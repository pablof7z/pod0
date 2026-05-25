//! `podcast-knowledge` — RAG-ready chunk store + semantic search baseline.
//!
//! M6.A baseline. The crate owns:
//!
//! - [`KnowledgeChunk`] / [`EmbeddingVector`] domain types.
//! - [`KnowledgeStore`] — in-memory chunk store with idempotent upsert.
//! - [`cosine_similarity`] / [`top_k_search`] — raw KNN primitive.
//! - [`actions`] — kernel-dispatched read/write actions.
//!
//! Persistence (LMDB + `nmp.vector.capability`) and hybrid ranking
//! (KNN + BM25 + RRF + reranker) arrive in M6.B; this baseline lets the
//! ingest pipeline land first.

pub mod actions;
pub mod search;
pub mod store;
pub mod types;

pub use actions::{IngestChunks, SearchKnowledge};
pub use search::{cosine_similarity, top_k_search};
pub use store::KnowledgeStore;
pub use types::{EmbeddingVector, KnowledgeChunk, SearchResult, TranscriptChunk};
