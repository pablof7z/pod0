use rusqlite::Connection;

use crate::{StorageError, schema_introspection::require_columns};

pub(crate) fn validate(connection: &Connection) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_workflow_configuration",
        &[
            "authority_state",
            "configuration_json",
            "created_at_ms",
            "origin",
            "revision",
            "schema_version",
            "singleton",
            "source_generation",
            "updated_at_ms",
        ],
    )?;
    require_columns(
        connection,
        "pod0_workflow_capability_snapshot",
        &[
            "observed_at_ms",
            "revision",
            "schema_version",
            "singleton",
            "snapshot_id",
            "snapshot_json",
        ],
    )
}
