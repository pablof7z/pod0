use rusqlite::Connection;

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_chapter_workflow_schema(
    connection: &Connection,
) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_publisher_chapter_workflows",
        &[
            "attempt",
            "cancellation_id",
            "command_id",
            "created_at_ms",
            "deadline_at_ms",
            "episode_id",
            "expected_selection_revision",
            "failure_code",
            "failure_detail",
            "generation",
            "issued_revision",
            "max_attempts",
            "not_before_ms",
            "request_id",
            "selected_artifact_id",
            "source_url",
            "source_version",
            "state",
            "updated_at_ms",
            "workflow_revision",
        ],
    )
}
