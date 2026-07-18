use std::path::Path;

use pod0_domain::{CommandId, ListeningDomainSnapshot};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::import_model::{InspectedLegacySource, LegacyBackupEvidence, ListeningImportReport};
use crate::listening_db_codec::{bool_value, i64_value, sleep};
use crate::listening_store_read::{read_snapshot, stored_import_report};
use crate::listening_store_write_entities::{
    insert_episodes, insert_podcasts, insert_subscriptions,
};
use crate::migration_db::{configure, open_connection, user_version};
use crate::model::command_id;
use crate::schema::validate_schema;
use crate::{CURRENT_SCHEMA_VERSION, StorageError};

pub(crate) fn write_import<F>(
    target_path: &Path,
    target_store_id: CommandId,
    import_id: CommandId,
    source: &InspectedLegacySource,
    backup: &LegacyBackupEvidence,
    verified_at_ms: i64,
    before_commit: F,
) -> Result<ListeningImportReport, StorageError>
where
    F: FnOnce() -> Result<(), StorageError>,
{
    let mut connection = open_connection(target_path, false)?;
    configure(&connection)?;
    let version = user_version(&connection)?;
    if version != CURRENT_SCHEMA_VERSION {
        return Err(StorageError::CorruptSchema {
            detail: "listening import target is not at the current schema",
        });
    }
    validate_schema(&connection, version)?;
    validate_store_identity(&connection, target_store_id)?;
    if let Some(existing) = stored_import_report(&connection, import_id, Some(backup))? {
        if existing.staged
            && existing.plan == source.plan
            && read_snapshot(&connection)? == source.snapshot
        {
            return Ok(ListeningImportReport {
                reused_existing: true,
                ..existing
            });
        }
        return Err(StorageError::ImportConflict);
    }
    if existing_import_count(&connection)? != 0 || target_has_listening_rows(&connection)? {
        return Err(StorageError::ImportConflict);
    }
    if let Some(state) = cutover_state(&connection)? {
        return if state == "authoritative" {
            Err(StorageError::CutoverAlreadyAuthoritative)
        } else {
            Err(StorageError::ImportConflict)
        };
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StorageError::sqlite("begin listening import", error))?;
    insert_import(&transaction, import_id, source, backup, verified_at_ms)?;
    insert_podcasts(&transaction, import_id, &source.snapshot)?;
    insert_subscriptions(&transaction, import_id, &source.snapshot)?;
    insert_episodes(&transaction, import_id, source)?;
    insert_playback(&transaction, import_id, &source.snapshot)?;
    stage_cutover(
        &transaction,
        &source.plan,
        source.snapshot.playback.revision.value,
        verified_at_ms,
    )?;
    if read_snapshot(&transaction)? != source.snapshot {
        return Err(StorageError::CorruptSchema {
            detail: "staged listening projection differs from source",
        });
    }
    before_commit()?;
    transaction
        .commit()
        .map_err(|error| StorageError::sqlite("commit listening import", error))?;
    Ok(ListeningImportReport {
        import_id,
        plan: source.plan.clone(),
        target_revision: source.snapshot.playback.revision.value,
        backup: backup.clone(),
        staged: true,
        reused_existing: false,
    })
}

fn insert_import(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    source: &InspectedLegacySource,
    backup: &LegacyBackupEvidence,
    verified_at_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO pod0_listening_imports(\
             import_id,source_kind,source_hash,source_generation,podcast_count,subscription_count,\
             episode_count,backup_byte_count,target_revision,state,verified_at_ms) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'verified',?10)",
            params![
                import_id.into_bytes().as_slice(),
                source.plan.source_kind.code(),
                source.plan.source_hash,
                i64_value(source.plan.source_generation, "source generation")?,
                source.plan.podcast_count,
                source.plan.subscription_count,
                source.plan.episode_count,
                i64_value(backup.byte_count, "backup byte count")?,
                i64_value(source.snapshot.playback.revision.value, "target revision")?,
                verified_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("record listening import", error))?;
    Ok(())
}

fn insert_playback(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    snapshot: &ListeningDomainSnapshot,
) -> Result<(), StorageError> {
    let playback = &snapshot.playback;
    let (sleep_code, sleep_duration, sleep_wire) = sleep(&playback.sleep_mode)?;
    transaction
        .execute(
            "INSERT INTO pod0_playback_state(singleton,active_episode_id,playback_rate_permille,\
         sleep_mode_code,sleep_duration_ms,sleep_wire_code,auto_mark_played_at_natural_end,\
         auto_play_next,state_revision,source_import_id) VALUES(1,?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                playback
                    .active_episode_id
                    .map(|id| id.into_bytes().to_vec()),
                playback.rate.value,
                sleep_code,
                sleep_duration,
                sleep_wire,
                bool_value(playback.auto_mark_played_at_natural_end),
                bool_value(playback.auto_play_next),
                i64_value(playback.revision.value, "playback revision")?,
                import_id.into_bytes().as_slice(),
            ],
        )
        .map_err(|error| StorageError::sqlite("insert playback state", error))?;
    Ok(())
}

fn stage_cutover(
    transaction: &Transaction<'_>,
    plan: &crate::LegacyImportPlan,
    revision: u64,
    at_ms: i64,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,core_revision,committed_at_ms) \
         VALUES('listening','staged',?1,?2,?3)",
        params![i64_value(plan.source_generation, "source generation")?, i64_value(revision, "revision")?, at_ms],
    ).map_err(|error| StorageError::sqlite("stage listening cutover", error))?;
    Ok(())
}

fn validate_store_identity(
    connection: &Connection,
    expected: CommandId,
) -> Result<(), StorageError> {
    let bytes: Vec<u8> = connection
        .query_row(
            "SELECT store_id FROM pod0_store_metadata WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read import target identity", error))?;
    if command_id(&bytes)? == expected {
        Ok(())
    } else {
        Err(StorageError::ImportConflict)
    }
}

fn existing_import_count(connection: &Connection) -> Result<u32, StorageError> {
    connection
        .query_row("SELECT COUNT(*) FROM pod0_listening_imports", [], |row| {
            row.get(0)
        })
        .map_err(|error| StorageError::sqlite("count listening imports", error))
}

fn target_has_listening_rows(connection: &Connection) -> Result<bool, StorageError> {
    let count: u32 = connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM pod0_podcasts)\
             +(SELECT COUNT(*) FROM pod0_subscriptions)\
             +(SELECT COUNT(*) FROM pod0_episodes)\
             +(SELECT COUNT(*) FROM pod0_playback_state)\
             +(SELECT COUNT(*) FROM pod0_queue_entries)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("inspect listening target", error))?;
    Ok(count != 0)
}

fn cutover_state(connection: &Connection) -> Result<Option<String>, StorageError> {
    connection
        .query_row(
            "SELECT state FROM pod0_domain_cutovers WHERE domain='listening'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read listening cutover", error))
}
