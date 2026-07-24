use rusqlite::Connection;

use crate::StorageError;
use crate::schema_introspection::require_columns;

pub(crate) fn validate_feed_discovery_schema(
    connection: &Connection,
    version: u32,
) -> Result<(), StorageError> {
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
    )?;
    if version < 30 {
        return Ok(());
    }
    require_columns(
        connection,
        "pod0_new_episode_notification_settings",
        &[
            "created_at_ms",
            "enabled",
            "revision",
            "schema_version",
            "singleton",
            "updated_at_ms",
        ],
    )?;
    require_columns(
        connection,
        "pod0_feed_discovery_workflows",
        &[
            "completed_at_ms",
            "expires_at_ms",
            "occurrence_id",
            "planned_at_ms",
            "stage",
            "updated_at_ms",
            "workflow_revision",
        ],
    )?;
    require_columns(
        connection,
        "pod0_feed_discovery_effects",
        &[
            "attempt",
            "cancellation_id",
            "command_id",
            "created_at_ms",
            "deadline_at_ms",
            "episode_id",
            "failure_code",
            "kind",
            "not_before_ms",
            "occurrence_id",
            "request_id",
            "stage",
            "updated_at_ms",
        ],
    )
}
