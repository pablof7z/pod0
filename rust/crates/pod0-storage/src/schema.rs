use pod0_domain::CommandId;
use rusqlite::{Connection, Transaction, params};

use crate::model::{APPLICATION_ID, StorageError};
use crate::schema_introspection::{require_columns, table_names};

const MIGRATION_1: &str = include_str!("../../../schema/migrations/0001_kernel_metadata.sql");
const MIGRATION_2: &str = include_str!("../../../schema/migrations/0002_migration_journal.sql");
const MIGRATION_3: &str = include_str!("../../../schema/migrations/0003_domain_cutovers.sql");
const MIGRATION_4: &str = include_str!("../../../schema/migrations/0004_listening_import.sql");
const MIGRATION_5: &str = include_str!("../../../schema/migrations/0005_library_runtime.sql");
const MIGRATION_6: &str = include_str!("../../../schema/migrations/0006_playback_runtime.sql");
const MIGRATION_7: &str = include_str!("../../../schema/migrations/0007_evidence_artifacts.sql");
const MIGRATION_8: &str = include_str!("../../../schema/migrations/0008_notes.sql");
const MIGRATION_9: &str = include_str!("../../../schema/migrations/0009_clips.sql");
const MIGRATION_10: &str = include_str!("../../../schema/migrations/0010_transcript_artifacts.sql");

pub(crate) fn migration_sql(version: u32) -> Option<&'static str> {
    match version {
        1 => Some(MIGRATION_1),
        2 => Some(MIGRATION_2),
        3 => Some(MIGRATION_3),
        4 => Some(MIGRATION_4),
        5 => Some(MIGRATION_5),
        6 => Some(MIGRATION_6),
        7 => Some(MIGRATION_7),
        8 => Some(MIGRATION_8),
        9 => Some(MIGRATION_9),
        10 => Some(MIGRATION_10),
        _ => None,
    }
}

pub(crate) fn apply_step(
    transaction: &Transaction<'_>,
    version: u32,
    observed_at_ms: i64,
    store_id: CommandId,
) -> Result<(), StorageError> {
    let sql = migration_sql(version).ok_or(StorageError::CorruptSchema {
        detail: "missing migration step",
    })?;
    transaction
        .execute_batch(sql)
        .map_err(|error| StorageError::sqlite("apply schema step", error))?;
    if version == 1 {
        transaction
            .execute(
                "INSERT INTO pod0_store_metadata(singleton,store_id) VALUES(1,?1)",
                [store_id.into_bytes().as_slice()],
            )
            .map_err(|error| StorageError::sqlite("record store identity", error))?;
    }
    transaction
        .pragma_update(None, "application_id", APPLICATION_ID)
        .map_err(|error| StorageError::sqlite("set application id", error))?;
    transaction
        .pragma_update(None, "user_version", version)
        .map_err(|error| StorageError::sqlite("set schema version", error))?;
    transaction
        .execute(
            "INSERT INTO pod0_schema_versions(component,version,updated_at_ms) VALUES('kernel',?1,?2) \
             ON CONFLICT(component) DO UPDATE SET version=excluded.version,updated_at_ms=excluded.updated_at_ms",
            params![version, observed_at_ms],
        )
        .map_err(|error| StorageError::sqlite("record component version", error))?;
    Ok(())
}

pub(crate) fn validate_schema(connection: &Connection, version: u32) -> Result<(), StorageError> {
    if version == 0 {
        let tables = table_names(connection)?;
        if tables.is_empty() {
            return Ok(());
        }
        return Err(StorageError::ForeignDatabase);
    }
    require_columns(
        connection,
        "pod0_schema_versions",
        &["component", "updated_at_ms", "version"],
    )?;
    require_columns(
        connection,
        "pod0_store_metadata",
        &["singleton", "store_id"],
    )?;
    let identity_count: u32 = connection
        .query_row("SELECT COUNT(*) FROM pod0_store_metadata", [], |row| {
            row.get(0)
        })
        .map_err(|error| StorageError::sqlite("validate store identity", error))?;
    if identity_count != 1 {
        return Err(StorageError::CorruptSchema {
            detail: "store identity must contain one row",
        });
    }
    let recorded: u32 = connection
        .query_row(
            "SELECT version FROM pod0_schema_versions WHERE component='kernel'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read component version", error))?;
    if recorded != version {
        return Err(StorageError::CorruptSchema {
            detail: "component and database versions differ",
        });
    }
    if version >= 2 {
        require_columns(
            connection,
            "pod0_migration_journal",
            &[
                "completed_at_ms",
                "diagnostic_code",
                "from_version",
                "migration_id",
                "started_at_ms",
                "state",
                "to_version",
            ],
        )?;
        require_columns(
            connection,
            "pod0_backup_evidence",
            &[
                "byte_count",
                "created_at_ms",
                "integrity_check",
                "migration_id",
                "page_count",
                "schema_version",
                "store_id",
            ],
        )?;
    }
    if version >= 3 {
        require_columns(
            connection,
            "pod0_domain_cutovers",
            &[
                "committed_at_ms",
                "core_revision",
                "domain",
                "source_generation",
                "state",
            ],
        )?;
    }
    if version >= 4 {
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
        require_columns(
            connection,
            "pod0_podcasts",
            &[
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
            ],
        )?;
        require_columns(
            connection,
            "pod0_subscriptions",
            &[
                "auto_download_code",
                "auto_download_latest_count",
                "auto_download_wire_code",
                "default_playback_rate_permille",
                "notifications_enabled",
                "podcast_id",
                "source_import_id",
                "subscribed_at_ms",
                "wifi_only",
            ],
        )?;
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
    }
    if version >= 5 {
        crate::schema_library::validate_library_schema(connection)?;
    }
    if version >= 7 {
        crate::schema_evidence::validate_evidence_schema(connection)?;
    }
    if version >= 8 {
        crate::schema_notes::validate_notes_schema(connection)?;
    }
    if version >= 9 {
        crate::schema_clips::validate_clips_schema(connection)?;
    }
    if version >= 10 {
        crate::schema_transcripts::validate_transcripts_schema(connection)?;
    }
    Ok(())
}
