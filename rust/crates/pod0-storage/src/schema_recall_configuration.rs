use rusqlite::Connection;

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_recall_configuration_schema(
    connection: &Connection,
) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_recall_configuration",
        &[
            "embedding_dimensions",
            "embedding_model",
            "embedding_provider",
            "embedding_space_digest",
            "origin",
            "reranker_enabled",
            "reranker_model",
            "reranker_provider",
            "revision",
            "schema_version",
            "singleton",
            "source_generation",
            "stored_embedding_model_id",
            "updated_at_ms",
        ],
    )
}
