use std::path::Path;

use pod0_domain::{CommandId, StateRevision};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use crate::StorageError;
use crate::legacy_transcript_db::orphan_transcript_podcast_id;
use crate::legacy_transcript_source::load_inspected_transcript_artifact;
use crate::migration_db::configure;
use crate::transcript_import_model::{
    InspectedTranscriptEntry, InspectedTranscriptSource, TranscriptBackupEvidence,
    TranscriptImportReport,
};
use crate::transcript_import_store_read::{open_current, read_import_report};
use crate::transcript_store_write_rows::{
    ensure_semantic_document, insert_or_validate_artifact, require_episode_parent,
};

pub(crate) fn write_transcript_import<F>(
    target_path: &Path,
    import_id: CommandId,
    source: &InspectedTranscriptSource,
    backup: &TranscriptBackupEvidence,
    staged_at_ms: i64,
    before_commit: F,
) -> Result<TranscriptImportReport, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    if staged_at_ms < 0 {
        return Err(StorageError::TranscriptImportConflict);
    }
    let mut connection = open_current(target_path)?;
    configure(&connection)?;
    if let Some(report) = read_import_report(&connection, import_id, true)? {
        return if report.plan == source.plan {
            Ok(report)
        } else {
            Err(StorageError::TranscriptImportConflict)
        };
    }
    let active: Option<Vec<u8>> = connection
        .query_row(
            "SELECT import_id FROM pod0_transcript_imports \
             WHERE state IN ('staged','verified','corrupt') LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read active transcript import", error))?;
    if active.is_some() {
        return Err(StorageError::TranscriptImportConflict);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StorageError::sqlite("begin transcript import", error))?;
    let target_revision = next_target_revision(&transaction)?;
    insert_import(
        &transaction,
        import_id,
        source,
        backup,
        target_revision,
        staged_at_ms,
    )?;
    for (offset, entry) in source.entries.iter().enumerate() {
        let index = u32::try_from(offset).map_err(|_| StorageError::ImportLimitExceeded {
            entity: "transcript artifacts",
        })?;
        let artifact = load_inspected_transcript_artifact(entry, index)?;
        stage_entry(&transaction, import_id, entry, &artifact, staged_at_ms)?;
    }
    let marker_changed = transaction
        .execute(
            "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,core_revision,committed_at_ms) \
             VALUES('transcripts','staged',?1,?2,?3) ON CONFLICT(domain) DO UPDATE SET \
             state='staged',source_generation=excluded.source_generation,\
             core_revision=excluded.core_revision,committed_at_ms=excluded.committed_at_ms \
             WHERE pod0_domain_cutovers.state='staged'",
            params![
                to_i64(source.plan.source_generation, "transcript source generation")?,
                to_i64(target_revision.value, "transcript target revision")?,
                staged_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("stage transcript cutover marker", error))?;
    if marker_changed != 1 {
        return Err(StorageError::CutoverAlreadyAuthoritative);
    }
    before_commit()?;
    transaction
        .commit()
        .map_err(|error| StorageError::sqlite("commit transcript import stage", error))?;
    let mut report = read_import_report(&connection, import_id, false)?
        .ok_or(StorageError::TranscriptImportNotFound)?;
    report.backup.reused_database = backup.reused_database;
    report.backup.reused_artifacts = backup.reused_artifacts;
    Ok(report)
}

fn insert_import(
    transaction: &rusqlite::Transaction<'_>,
    import_id: CommandId,
    source: &InspectedTranscriptSource,
    backup: &TranscriptBackupEvidence,
    target_revision: StateRevision,
    staged_at_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO pod0_transcript_imports(import_id,source_kind,source_schema_version,\
             source_generation,source_selection_digest,source_database_digest,backup_database_digest,\
             backup_database_byte_count,artifact_count,selected_count,target_revision,state,staged_at_ms) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'staged',?12)",
            params![
                import_id.into_bytes().as_slice(), source.plan.source_kind.code(),
                source.plan.source_kind.schema_version(),
                to_i64(source.plan.source_generation, "transcript source generation")?,
                source.plan.source_selection_digest.into_bytes().as_slice(),
                source.plan.source_database_digest.into_bytes().as_slice(),
                backup.database_digest.into_bytes().as_slice(),
                to_i64(backup.database_byte_count, "transcript database backup bytes")?,
                source.plan.artifact_count,
                source.plan.selected_count,
                to_i64(target_revision.value, "transcript target revision")?,
                staged_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("record transcript import", error))?;
    Ok(())
}

fn stage_entry(
    transaction: &rusqlite::Transaction<'_>,
    import_id: CommandId,
    entry: &InspectedTranscriptEntry,
    artifact: &pod0_domain::TranscriptArtifact,
    staged_at_ms: i64,
) -> Result<(), StorageError> {
    if entry.is_orphan {
        ensure_orphan_parent(transaction, artifact, staged_at_ms)?;
    }
    require_episode_parent(transaction, artifact)?;
    ensure_semantic_document(transaction, artifact)?;
    insert_or_validate_artifact(transaction, artifact, Some(import_id), staged_at_ms)?;
    transaction
        .execute(
            "INSERT INTO pod0_transcript_import_entries(import_id,episode_id,legacy_row_id,\
             legacy_schema_version,legacy_input_version,legacy_output_version,legacy_origin,\
             legacy_integrity,legacy_verified_at_ms,is_selected,selected_row_digest,\
             selected_file_digest,backup_file_digest,backup_file_byte_count,artifact_id,\
             transcript_version_id) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            params![
                import_id.into_bytes().as_slice(),
                entry.episode_id.into_bytes().as_slice(),
                to_i64(entry.legacy_row_id, "transcript legacy row identity")?,
                entry.legacy_schema_version,
                entry.legacy_input_version,
                entry.legacy_output_version,
                entry.legacy_origin.as_deref().unwrap_or("unknown"),
                entry.legacy_integrity,
                entry.legacy_verified_at_ms,
                entry.is_selected,
                entry.selected_row_digest.into_bytes().as_slice(),
                entry.selected_file_digest.into_bytes().as_slice(),
                entry.selected_file_digest.into_bytes().as_slice(),
                to_i64(
                    entry.selected_file_byte_count,
                    "transcript artifact backup bytes"
                )?,
                entry.artifact_id.into_bytes().as_slice(),
                entry.transcript_version_id.into_bytes().as_slice(),
            ],
        )
        .map_err(|error| StorageError::sqlite("record transcript import entry", error))?;
    Ok(())
}

fn ensure_orphan_parent(
    transaction: &rusqlite::Transaction<'_>,
    artifact: &pod0_domain::TranscriptArtifact,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    if artifact.podcast_id != orphan_transcript_podcast_id() {
        return Err(StorageError::InvalidTranscriptArtifact);
    }
    let source_import_id: Vec<u8> = transaction
        .query_row(
            "SELECT import_id FROM pod0_listening_imports ORDER BY verified_at_ms DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read orphan listening import", error))?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO pod0_podcasts(podcast_id,kind_code,kind_wire_code,feed_url,\
             feed_key_v1,title,author,image_url,description,language,categories_json,\
             discovered_at_ms,title_is_placeholder,last_refreshed_at_ms,etag,last_modified,\
             source_import_id,library_visible) VALUES(?1,2,NULL,NULL,NULL,'Recovered transcripts',\
             '',NULL,'Transcripts retained after their library episode disappeared.',NULL,'[]',\
             ?2,1,NULL,NULL,NULL,?3,0)",
            params![
                artifact.podcast_id.into_bytes().as_slice(),
                observed_at_ms,
                source_import_id.as_slice(),
            ],
        )
        .map_err(|error| StorageError::sqlite("create orphan transcript podcast", error))?;
    let episode_key = hex_id(artifact.episode_id.into_bytes());
    transaction
        .execute(
            "INSERT OR IGNORE INTO pod0_episodes(episode_id,podcast_id,publisher_guid,title,\
             description,published_at_ms,duration_ms,enclosure_url,enclosure_mime_type,image_url,\
             resume_position_ms,completion_code,completion_cause_code,completion_cause_wire_code,\
             is_starred,download_code,download_wire_code,download_ref_version,download_ref_key,\
             download_byte_count,transcript_code,transcript_wire_code,transcript_ref_version,\
             transcript_ref_key,transcript_source_code,transcript_source_wire_code,legacy_payload,\
             source_import_id) VALUES(?1,?2,?3,'Recovered transcript','',0,NULL,?4,NULL,NULL,0,1,\
             NULL,NULL,0,1,NULL,NULL,NULL,NULL,1,NULL,NULL,NULL,NULL,NULL,X'7B7D',?5)",
            params![
                artifact.episode_id.into_bytes().as_slice(),
                artifact.podcast_id.into_bytes().as_slice(),
                format!("orphan-transcript:{episode_key}"),
                format!("pod0-orphan-transcript://{episode_key}"),
                source_import_id.as_slice(),
            ],
        )
        .map_err(|error| StorageError::sqlite("create orphan transcript episode", error))?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO pod0_episode_feed_metadata(episode_id,persons_json,\
             sound_bites_json) VALUES(?1,'[]','[]')",
            [artifact.episode_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("create orphan transcript metadata", error))?;
    Ok(())
}

fn hex_id(bytes: [u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn next_target_revision(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<StateRevision, StorageError> {
    let current: i64 = transaction
        .query_row(
            "SELECT collection_revision FROM pod0_transcript_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read transcript import revision", error))?;
    let current = u64::try_from(current).map_err(|_| StorageError::TranscriptImportConflict)?;
    Ok(StateRevision::new(
        current
            .checked_add(1)
            .ok_or(StorageError::TranscriptImportConflict)?,
    ))
}

fn to_i64(value: u64, _: &'static str) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::TranscriptImportConflict)
}
