use rusqlite::Connection;

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_agent_generated_audio_schema(
    connection: &Connection,
) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_agent_generated_audio_artifacts",
        &[
            "artifact_id",
            "commit_id",
            "committed_at_ms",
            "conversation_id",
            "episode_id",
            "media_byte_count",
            "media_content_digest",
            "media_type",
            "media_url",
            "model_reference",
            "podcast_id",
            "proposal_id",
            "script_content_digest",
            "turn_id",
            "voice_id",
        ],
    )
}
