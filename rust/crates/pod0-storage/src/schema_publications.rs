use rusqlite::Connection;

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_publication_schema(connection: &Connection) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_publications",
        &[
            "artifact_id",
            "artifact_kind_code",
            "artifact_kind_wire_code",
            "correlation_token",
            "episode_id",
            "event_id_hex",
            "expected_author_hex",
            "media_byte_count",
            "media_content_digest",
            "media_type",
            "podcast_id",
            "prepared_at_ms",
            "public_media_url",
            "publication_id",
            "receipt_id",
            "semantic_revision",
            "stage_code",
            "state_revision",
            "updated_at_ms",
        ],
    )?;
    require_columns(
        connection,
        "pod0_publication_facts",
        &[
            "attempt",
            "detail",
            "event_id_hex",
            "fact_digest",
            "fact_kind_code",
            "observed_at_ms",
            "publication_id",
            "route_id",
            "sequence_number",
        ],
    )?;
    require_columns(
        connection,
        "pod0_publication_commands",
        &[
            "command_fingerprint",
            "command_id",
            "completed_at_ms",
            "publication_id",
        ],
    )
}
