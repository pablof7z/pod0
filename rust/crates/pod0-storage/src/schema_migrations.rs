use pod0_domain::CommandId;
use rusqlite::{Transaction, params};

use crate::model::{APPLICATION_ID, StorageError};

const MIGRATIONS: &[&str] = &[
    include_str!("../../../schema/migrations/0001_kernel_metadata.sql"),
    include_str!("../../../schema/migrations/0002_migration_journal.sql"),
    include_str!("../../../schema/migrations/0003_domain_cutovers.sql"),
    include_str!("../../../schema/migrations/0004_listening_import.sql"),
    include_str!("../../../schema/migrations/0005_library_runtime.sql"),
    include_str!("../../../schema/migrations/0006_playback_runtime.sql"),
    include_str!("../../../schema/migrations/0007_evidence_artifacts.sql"),
    include_str!("../../../schema/migrations/0008_notes.sql"),
    include_str!("../../../schema/migrations/0009_clips.sql"),
    include_str!("../../../schema/migrations/0010_transcript_artifacts.sql"),
    include_str!("../../../schema/migrations/0011_retained_library_artifacts.sql"),
    include_str!("../../../schema/migrations/0012_complete_transcript_history.sql"),
    include_str!("../../../schema/migrations/0013_chapter_artifacts.sql"),
    include_str!("../../../schema/migrations/0014_chapter_authority.sql"),
    include_str!("../../../schema/migrations/0015_chapter_publisher_workflows.sql"),
    include_str!("../../../schema/migrations/0016_chapter_model_workflows.sql"),
    include_str!("../../../schema/migrations/0017_model_chapter_completion_history.sql"),
    include_str!("../../../schema/migrations/0018_recall_configuration.sql"),
    include_str!("../../../schema/migrations/0019_download_workflows.sql"),
    include_str!("../../../schema/migrations/0020_transcript_workflows.sql"),
    include_str!("../../../schema/migrations/0021_scheduled_agent_workflows.sql"),
    include_str!("../../../schema/migrations/0022_scheduled_agent_cutover_evidence.sql"),
    include_str!("../../../schema/migrations/0023_agent_turns.sql"),
    include_str!("../../../schema/migrations/0024_agent_generated_audio.sql"),
    include_str!("../../../schema/migrations/0025_nmp_publications.sql"),
    include_str!("../../../schema/migrations/0026_nostr_signer_state.sql"),
    include_str!("../../../schema/migrations/0027_agent_history_cutover.sql"),
];

pub(crate) fn migration_sql(version: u32) -> Option<&'static str> {
    let index = usize::try_from(version.checked_sub(1)?).ok()?;
    MIGRATIONS.get(index).copied()
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
