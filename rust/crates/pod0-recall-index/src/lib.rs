#![deny(unsafe_code)]

mod cache;
mod identity;
mod migration;
mod model;
mod query;
mod readiness;
mod schema;
mod store;
mod store_write;

pub use model::recall_index_path_for_core_store;
pub use model::{
    MAX_RECALL_EMBEDDING_BATCH, RECALL_INDEX_DIMENSIONS, RECALL_INDEX_SCHEMA_VERSION,
    RecallCancellation, RecallEmbeddingRequest, RecallIndex, RecallIndexCandidate,
    RecallIndexCutoverReceipt, RecallIndexError, RecallIndexPlan, RecallIndexSpan,
    RecallSpanEmbedding,
};

#[cfg(test)]
mod cancellation_tests;
#[cfg(test)]
mod migration_tests;
#[cfg(test)]
mod recovery_tests;
#[cfg(test)]
mod tests;
