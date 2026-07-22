use rusqlite::Connection;

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_agent_schema(connection: &Connection) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_agent_turns",
        &[
            "conversation_id",
            "created_at_ms",
            "stage",
            "state_digest",
            "state_json",
            "state_revision",
            "state_schema_version",
            "turn_id",
            "updated_at_ms",
        ],
    )?;
    require_columns(
        connection,
        "pod0_agent_audit",
        &[
            "event_kind",
            "observed_at_ms",
            "state_digest",
            "turn_id",
            "turn_revision",
        ],
    )?;
    require_columns(
        connection,
        "pod0_agent_command_receipts",
        &[
            "applied_revision",
            "command_fingerprint",
            "command_id",
            "completed_at_ms",
            "turn_id",
        ],
    )
}
