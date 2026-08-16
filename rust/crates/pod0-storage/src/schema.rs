use rusqlite::Connection;

use crate::model::StorageError;
use crate::schema_introspection::{require_columns, table_names};
pub(crate) use crate::schema_migrations::apply_step;

#[path = "schema_library_network.rs"]
mod library_network;
#[path = "schema_listening.rs"]
mod listening;
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
        listening::validate_listening_schema(connection, version)?;
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
    if version >= 27 {
        crate::schema_agent::validate_agent_history_cutover_schema(connection)?;
    }
    if version >= 28 {
        crate::schema_memories::validate_memory_schema(connection)?;
    }
    if version >= 29 {
        crate::schema_feed_discoveries::validate_feed_discovery_schema(connection, version)?;
    }
    if version >= 8 {
        crate::schema_notes::validate_notes_schema(connection, version)?;
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
    if version >= 34 {
        crate::schema_categories::validate_category_schema(connection)?;
    }
    if version >= 35 {
        crate::schema_speakers::validate_speaker_schema(connection)?;
    }
    if version >= 37 {
        crate::schema_activity::validate_activity_schema(connection)?;
    }
    if version >= 38 {
        require_columns(
            connection,
            "pod0_recall_queries",
            &[
                "cancellation_id",
                "command_id",
                "created_at_ms",
                "evidence_json",
                "failure_json",
                "query_id",
                "query_json",
                "revision",
                "stage_json",
                "updated_at_ms",
            ],
        )?;
        require_columns(
            connection,
            "pod0_recall_index_cutover_workflow",
            &[
                "cancellation_id",
                "command_id",
                "removed_file_count",
                "revision",
                "singleton",
                "stage",
                "updated_at_ms",
            ],
        )?;
    }
    if version >= 40 {
        require_columns(
            connection,
            "pod0_legacy_effect_recovery_v40",
            &[
                "intent_id",
                "prior_fence",
                "prior_intent_state_code",
                "recovery_activity_id",
                "recovery_code",
            ],
        )?;
    }
    if version >= 41 {
        crate::schema_workflow_configuration::validate(connection)?;
    }
    if version >= 43 {
        library_network::validate(connection)?;
    }
    Ok(())
}
