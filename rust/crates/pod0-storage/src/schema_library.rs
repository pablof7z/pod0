use rusqlite::Connection;

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_library_schema(connection: &Connection) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_episode_feed_metadata",
        &[
            "chapters_url",
            "episode_id",
            "persons_json",
            "publisher_transcript_format_code",
            "publisher_transcript_format_wire_code",
            "publisher_transcript_media_type",
            "publisher_transcript_url",
            "sound_bites_json",
        ],
    )?;
    require_columns(
        connection,
        "pod0_library_commands",
        &[
            "applied_revision",
            "command_fingerprint",
            "command_id",
            "completed_at_ms",
        ],
    )?;
    Ok(())
}
