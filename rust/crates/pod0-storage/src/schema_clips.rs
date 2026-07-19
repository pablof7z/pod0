use rusqlite::Connection;

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_clips_schema(connection: &Connection) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_clip_imports",
        &[
            "backup_byte_count",
            "clip_count",
            "import_id",
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
        "pod0_clip_state",
        &["collection_revision", "singleton", "source_import_id"],
    )?;
    require_columns(
        connection,
        "pod0_clips",
        &[
            "caption",
            "clip_id",
            "clip_revision",
            "created_at_ms",
            "created_command_id",
            "deleted",
            "end_ms",
            "episode_id",
            "evidence_content_digest",
            "evidence_generation_id",
            "evidence_span_id",
            "evidence_transcript_version_id",
            "frozen_transcript_text",
            "podcast_id",
            "source_code",
            "source_import_id",
            "source_wire_code",
            "speaker_id",
            "speaker_label",
            "start_ms",
        ],
    )
}
