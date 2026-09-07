use rusqlite::Connection;

use crate::{StorageError, schema_introspection::require_columns};

pub(crate) fn validate(connection: &Connection) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_product_settings",
        &[
            "created_at_ms",
            "revision",
            "schema_version",
            "singleton",
            "updated_at_ms",
            "values_json",
            "writer_counter",
            "writer_id",
        ],
    )?;
    require_columns(
        connection,
        "pod0_settings_validation_evidence",
        &[
            "candidate_counter",
            "candidate_schema_version",
            "command_id",
            "observed_at_ms",
            "source_code",
            "validation_json",
            "writer_id",
        ],
    )?;
    require_columns(
        connection,
        "pod0_settings_sync_conflicts",
        &[
            "candidate_counter",
            "candidate_digest",
            "candidate_writer_id",
            "command_id",
            "current_counter",
            "current_digest",
            "current_writer_id",
            "observed_at_ms",
            "winner_code",
        ],
    )
}
