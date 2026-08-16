use rusqlite::Connection;

use crate::{StorageError, schema_introspection::require_columns};

pub(crate) fn validate(connection: &Connection) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_library_network_workflows",
        &[
            "cancellation_id",
            "command_fingerprint",
            "command_id",
            "created_at_ms",
            "failure_code",
            "intent_json",
            "pending_request_id",
            "pending_step_json",
            "result_json",
            "revision",
            "stage",
            "updated_at_ms",
        ],
    )
}
