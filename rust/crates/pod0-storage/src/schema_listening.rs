use rusqlite::Connection;

use crate::model::StorageError;
use crate::schema_introspection::require_columns;

pub(super) fn validate_listening_schema(
    connection: &Connection,
    version: u32,
) -> Result<(), StorageError> {
    require_columns(
        connection,
        "pod0_listening_imports",
        &[
            "backup_byte_count",
            "episode_count",
            "import_id",
            "podcast_count",
            "source_generation",
            "source_hash",
            "source_kind",
            "state",
            "subscription_count",
            "target_revision",
            "verified_at_ms",
        ],
    )?;
    let mut podcast_columns = vec![
        "author",
        "categories_json",
        "description",
        "discovered_at_ms",
        "etag",
        "feed_key_v1",
        "feed_url",
        "image_url",
        "kind_code",
        "kind_wire_code",
        "language",
        "last_modified",
        "last_refreshed_at_ms",
        "podcast_id",
        "source_import_id",
        "title",
        "title_is_placeholder",
    ];
    if version >= 11 {
        podcast_columns.push("library_visible");
    }
    require_columns(connection, "pod0_podcasts", &podcast_columns)?;
    let mut subscription_columns = vec![
        "auto_download_code",
        "auto_download_latest_count",
        "auto_download_wire_code",
        "default_playback_rate_permille",
        "notifications_enabled",
        "podcast_id",
        "source_import_id",
        "subscribed_at_ms",
        "wifi_only",
    ];
    if version >= 32 {
        subscription_columns.extend([
            "transcript_start_policy_code",
            "transcript_start_policy_wire_code",
        ]);
    }
    require_columns(connection, "pod0_subscriptions", &subscription_columns)?;
    require_columns(
        connection,
        "pod0_episodes",
        &[
            "completion_cause_code",
            "completion_cause_wire_code",
            "completion_code",
            "description",
            "download_byte_count",
            "download_code",
            "download_ref_key",
            "download_ref_version",
            "download_wire_code",
            "duration_ms",
            "enclosure_mime_type",
            "enclosure_url",
            "episode_id",
            "image_url",
            "is_starred",
            "legacy_payload",
            "podcast_id",
            "published_at_ms",
            "publisher_guid",
            "resume_position_ms",
            "source_import_id",
            "title",
            "transcript_code",
            "transcript_ref_key",
            "transcript_ref_version",
            "transcript_source_code",
            "transcript_source_wire_code",
            "transcript_wire_code",
        ],
    )?;
    let mut playback_columns = vec![
        "active_episode_id",
        "auto_mark_played_at_natural_end",
        "auto_play_next",
        "playback_rate_permille",
        "singleton",
        "sleep_duration_ms",
        "sleep_mode_code",
        "sleep_wire_code",
        "source_import_id",
        "state_revision",
    ];
    if version >= 6 {
        playback_columns.extend([
            "active_segment_end_ms",
            "active_segment_label",
            "active_segment_start_ms",
            "last_position_committed_at_ms",
        ]);
    }
    require_columns(connection, "pod0_playback_state", &playback_columns)?;
    require_columns(
        connection,
        "pod0_queue_entries",
        &[
            "episode_id",
            "label",
            "queue_entry_id",
            "segment_end_ms",
            "segment_start_ms",
            "sort_order",
            "source_import_id",
        ],
    )?;
    Ok(())
}
