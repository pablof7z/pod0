use rusqlite::Connection;

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_notes_schema(connection: &Connection) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_note_imports",
        &[
            "backup_byte_count",
            "import_id",
            "note_count",
            "source_generation",
            "source_hash",
            "source_kind",
            "state",
            "target_revision",
            "verified_at_ms",
        ],
    )?;
    require_columns(
        connection,
        "pod0_note_state",
        &["collection_revision", "singleton", "source_import_id"],
    )?;
    require_columns(
        connection,
        "pod0_notes",
        &[
            "author_code",
            "author_wire_code",
            "created_at_ms",
            "created_command_id",
            "deleted",
            "episode_id",
            "evidence_content_digest",
            "evidence_generation_id",
            "evidence_span_id",
            "evidence_transcript_version_id",
            "kind_code",
            "kind_wire_code",
            "note_id",
            "note_revision",
            "position_ms",
            "source_import_id",
            "target_code",
            "target_note_id",
            "target_wire_code",
            "text",
        ],
    )
}
