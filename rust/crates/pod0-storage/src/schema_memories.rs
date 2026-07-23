use rusqlite::Connection;

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_memory_schema(connection: &Connection) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_memory_state",
        &[
            "authority_active",
            "collection_revision",
            "singleton",
            "source_generation",
        ],
    )?;
    require_columns(
        connection,
        "pod0_memories",
        &[
            "content",
            "created_at_ms",
            "created_command_id",
            "deleted",
            "memory_id",
            "memory_revision",
            "source_code",
            "updated_at_ms",
        ],
    )?;
    require_columns(
        connection,
        "pod0_compiled_memory",
        &["compiled_at_ms", "singleton", "text"],
    )?;
    require_columns(
        connection,
        "pod0_compiled_memory_sources",
        &["memory_id", "singleton", "sort_order"],
    )?;
    require_columns(
        connection,
        "pod0_memory_cutover_evidence",
        &[
            "backup_byte_count",
            "backup_digest",
            "committed_at_ms",
            "compiled_present",
            "deleted_count",
            "memory_count",
            "singleton",
            "source_fingerprint",
            "source_generation",
            "staged_at_ms",
            "state",
            "verified_at_ms",
        ],
    )
}
