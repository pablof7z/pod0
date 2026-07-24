use rusqlite::Connection;

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_feed_discovery_schema(connection: &Connection) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_feed_discovery_occurrences",
        &[
            "command_id",
            "created_at_ms",
            "is_initial_population",
            "item_count",
            "observed_at_ms",
            "occurrence_id",
            "podcast_id",
            "policy_version",
            "state",
            "updated_at_ms",
            "workflow_schema_version",
        ],
    )?;
    require_columns(
        connection,
        "pod0_feed_discovery_items",
        &[
            "episode_id",
            "input_version",
            "item_id",
            "occurrence_id",
            "published_at_ms",
        ],
    )?;
    require_columns(
        connection,
        "pod0_feed_apply_receipts",
        &[
            "command_id",
            "discovery_occurrence_id",
            "inserted_episode_count",
            "podcast_id",
        ],
    )
}
