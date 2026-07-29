use rusqlite::Connection;

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_speaker_schema(connection: &Connection) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_speakers",
        &[
            "created_at_ms",
            "created_command_id",
            "deleted",
            "display_name",
            "speaker_entity_id",
            "speaker_entity_revision",
            "updated_at_ms",
        ],
    )?;
    require_columns(
        connection,
        "pod0_speaker_assignments",
        &[
            "artifact_id",
            "confidence",
            "decided_at_ms",
            "decided_command_id",
            "origin_code",
            "speaker_entity_id",
            "speaker_id",
        ],
    )
}
