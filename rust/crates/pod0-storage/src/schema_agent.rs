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

pub(crate) fn validate_agent_history_cutover_schema(
    connection: &Connection,
) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_agent_history_cutover_evidence",
        &[
            "backup_byte_count",
            "backup_digest",
            "committed_at_ms",
            "conversation_count",
            "message_count",
            "singleton",
            "source_fingerprint",
            "source_generation",
            "staged_at_ms",
            "state",
            "turn_count",
            "verified_at_ms",
        ],
    )?;
    require_columns(
        connection,
        "pod0_agent_history_staged_conversations",
        &["conversation_id", "created_at_ms", "title", "updated_at_ms"],
    )?;
    require_columns(
        connection,
        "pod0_agent_history_staged_turns",
        &[
            "conversation_id",
            "created_at_ms",
            "state_digest",
            "state_json",
            "state_schema_version",
            "turn_id",
            "updated_at_ms",
        ],
    )?;
    require_columns(
        connection,
        "pod0_agent_conversation_metadata",
        &[
            "conversation_id",
            "created_at_ms",
            "source",
            "title",
            "updated_at_ms",
        ],
    )
}
