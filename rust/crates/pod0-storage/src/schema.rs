use rusqlite::Connection;

use crate::model::StorageError;
use crate::schema_introspection::{require_columns, table_names};
pub(crate) use crate::schema_migrations::apply_step;

#[path = "schema_recall_configuration.rs"]
mod recall_configuration;

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
    if version >= 18 {
        recall_configuration::validate_recall_configuration_schema(connection)?;
    }
    if version >= 19 {
        crate::schema_download_workflows::validate_download_workflow_schema(connection)?;
    }
    if version >= 20 {
        crate::schema_transcript_workflows::validate_transcript_workflow_schema(connection)?;
    }
    if version >= 21 {
        crate::schema_scheduled_agent::validate_scheduled_agent_schema(connection)?;
    }
    if version >= 22 {
        crate::schema_scheduled_agent::validate_scheduled_agent_cutover_schema(connection)?;
    }
    if version >= 23 {
        crate::schema_agent::validate_agent_schema(connection)?;
    }
    if version >= 24 {
        crate::agent_generated_audio_store::schema::validate_agent_generated_audio_schema(
            connection,
        )?;
    }
    if version >= 25 {
        crate::schema_publications::validate_publication_schema(connection)?;
    }
    if version >= 26 {
        crate::schema_signer::validate_signer_schema(connection)?;
    }
    if version >= 27 {
        crate::schema_agent::validate_agent_history_cutover_schema(connection)?;
    }
    if version >= 28 {
        crate::schema_memories::validate_memory_schema(connection)?;
    }
    if version >= 8 {
        crate::schema_notes::validate_notes_schema(connection)?;
    }
    if version >= 9 {
        crate::schema_clips::validate_clips_schema(connection)?;
    }
    if version >= 10 {
        crate::schema_transcripts::validate_transcripts_schema(connection, version)?;
    }
    if version >= 13 {
        crate::schema_chapters::validate_chapters_schema(connection, version)?;
    }
    if version >= 15 {
        crate::schema_chapter_workflows::validate_chapter_workflow_schema(connection)?;
    }
    if version >= 16 {
        crate::schema_model_chapter_workflows::validate_model_chapter_workflow_schema(connection)?;
    }
    Ok(())
}
