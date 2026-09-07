use std::path::Path;

use pod0_domain::CommandId;
use rusqlite::{TransactionBehavior, params};
use sha2::{Digest as _, Sha256};

use crate::migration_db::{configure, open_connection};
use crate::{
    CURRENT_SCHEMA_VERSION, CoreStoreMigrator, LibraryStore, MigrationClock, StorageError,
};

/// Creates a new, empty store whose Rust-owned domains are authoritative.
///
/// This is the production bootstrap path for a new installation. It refuses
/// existing paths so it can never reinterpret a migration target as empty.
pub fn create_authoritative_store(
    path: &Path,
    store_id: CommandId,
    observed_at_ms: i64,
) -> Result<LibraryStore, StorageError> {
    if path.exists() {
        return Err(StorageError::ImportConflict);
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|error| StorageError::io("create authoritative store parent", error))?;
    let staging = tempfile::Builder::new()
        .prefix(".pod0-bootstrap-")
        .tempdir_in(parent)
        .map_err(|error| StorageError::io("create authoritative store staging", error))?;
    let staged_path = staging.path().join("pod0.sqlite");
    let backup_path = staging.path().join("migration-backup.sqlite");
    let observed_at_ms = observed_at_ms.max(0);
    CoreStoreMigrator::new(BootstrapClock(observed_at_ms)).migrate(
        &staged_path,
        CURRENT_SCHEMA_VERSION,
        &backup_path,
        store_id,
    )?;
    establish_empty_authority(&staged_path, store_id, observed_at_ms)?;
    LibraryStore::open_authoritative(&staged_path)?;
    std::fs::File::open(&staged_path)
        .and_then(|file| file.sync_all())
        .map_err(|error| StorageError::io("sync authoritative store staging", error))?;
    std::fs::hard_link(&staged_path, path)
        .map_err(|error| StorageError::io("publish authoritative store", error))?;
    crate::user_data_erasure_marker::sync_parent(path)?;
    LibraryStore::open_authoritative(path)
}

fn establish_empty_authority(
    path: &Path,
    store_id: CommandId,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let mut connection = open_connection(path, false)?;
    configure(&connection)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| StorageError::sqlite("begin authoritative bootstrap", error))?;
    let listening_id = derived_id(store_id, b"listening");
    let notes_id = derived_id(store_id, b"notes");
    let clips_id = derived_id(store_id, b"clips");
    let chapters_id = derived_id(store_id, b"chapters");

    seed_listening(&transaction, listening_id, observed_at_ms)?;
    seed_artifact_domain(&transaction, "notes", notes_id, observed_at_ms)?;
    seed_artifact_domain(&transaction, "clips", clips_id, observed_at_ms)?;
    transaction
        .execute(
            "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,core_revision,\
             committed_at_ms) VALUES('transcripts','authoritative',1,0,?1)",
            [observed_at_ms],
        )
        .map_err(|error| StorageError::sqlite("bootstrap transcript authority", error))?;
    seed_chapters(&transaction, chapters_id, observed_at_ms)?;
    seed_runtime_authorities(&transaction, observed_at_ms)?;
    transaction
        .commit()
        .map_err(|error| StorageError::sqlite("commit authoritative bootstrap", error))
}

fn seed_listening(
    transaction: &rusqlite::Transaction<'_>,
    import_id: CommandId,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO pod0_listening_imports(import_id,source_kind,source_hash,\
             source_generation,podcast_count,subscription_count,episode_count,backup_byte_count,\
             target_revision,state,verified_at_ms) \
             VALUES(?1,2,?2,1,0,0,0,1,1,'verified',?3)",
            params![
                import_id.into_bytes().as_slice(),
                digest_hex(b"pod0:fresh-store:listening:v1"),
                observed_at_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("bootstrap listening provenance", error))?;
    transaction
        .execute(
            "INSERT INTO pod0_playback_state(singleton,active_episode_id,\
             playback_rate_permille,sleep_mode_code,sleep_duration_ms,sleep_wire_code,\
             auto_mark_played_at_natural_end,auto_play_next,state_revision,source_import_id) \
             VALUES(1,NULL,1000,1,NULL,NULL,1,1,1,?1)",
            [import_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("bootstrap playback state", error))?;
    transaction
        .execute(
            "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,core_revision,\
             committed_at_ms) VALUES('listening','authoritative',1,1,?1)",
            [observed_at_ms],
        )
        .map_err(|error| StorageError::sqlite("bootstrap listening authority", error))?;
    Ok(())
}

fn seed_artifact_domain(
    transaction: &rusqlite::Transaction<'_>,
    domain: &str,
    import_id: CommandId,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let (imports, state, count_column) = match domain {
        "notes" => ("pod0_note_imports", "pod0_note_state", "note_count"),
        "clips" => ("pod0_clip_imports", "pod0_clip_state", "clip_count"),
        _ => return Err(StorageError::InvalidActivity),
    };
    let insert_import = format!(
        "INSERT INTO {imports}(import_id,source_kind,source_hash,source_generation,{count_column},\
         backup_byte_count,target_revision,state,verified_at_ms) \
         VALUES(?1,2,?2,1,0,1,1,'verified',?3)"
    );
    transaction
        .execute(
            &insert_import,
            params![
                import_id.into_bytes().as_slice(),
                digest_hex(format!("pod0:fresh-store:{domain}:v1").as_bytes()),
                observed_at_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("bootstrap artifact provenance", error))?;
    transaction
        .execute(
            &format!(
                "INSERT INTO {state}(singleton,collection_revision,source_import_id) \
                 VALUES(1,1,?1)"
            ),
            [import_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("bootstrap artifact state", error))?;
    transaction
        .execute(
            "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,core_revision,\
             committed_at_ms) VALUES(?1,'authoritative',1,1,?2)",
            params![domain, observed_at_ms],
        )
        .map_err(|error| StorageError::sqlite("bootstrap artifact authority", error))?;
    Ok(())
}

fn seed_chapters(
    transaction: &rusqlite::Transaction<'_>,
    import_id: CommandId,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let digest = digest(b"pod0:fresh-store:chapters:v1");
    transaction
        .execute(
            "INSERT INTO pod0_chapter_imports(import_id,source_kind,source_identity,\
             source_generation,source_byte_count,source_database_digest,source_selection_digest,\
             command_fingerprint,evidence_count,artifact_count,selected_count,blocked_count,\
             chapter_count,ad_span_count,target_revision,state,backup_database_digest,\
             backup_database_byte_count,backup_file_count,backup_file_byte_count,staged_at_ms,\
             verified_at_ms,imported_at_ms,discarded_at_ms,diagnostic_code) \
             VALUES(?1,'artifact_sqlite_v1',?2,1,0,?2,?2,?2,0,0,0,0,0,0,1,'imported',\
             ?2,0,0,0,?3,?3,?3,NULL,NULL)",
            params![
                import_id.into_bytes().as_slice(),
                digest.as_slice(),
                observed_at_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("bootstrap chapter provenance", error))?;
    transaction
        .execute(
            "UPDATE pod0_chapter_state SET authority_active=1,authority_import_id=?1 \
             WHERE singleton=1 AND authority_active=0",
            [import_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("bootstrap chapter authority", error))?;
    Ok(())
}

fn seed_runtime_authorities(
    transaction: &rusqlite::Transaction<'_>,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_memory_state SET authority_active=1,source_generation=1 \
             WHERE singleton=1 AND authority_active=0",
            [],
        )
        .map_err(|error| StorageError::sqlite("bootstrap memory authority", error))?;
    transaction
        .execute(
            "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,core_revision,\
             committed_at_ms) VALUES('downloads','authoritative',1,1,?1)",
            [observed_at_ms],
        )
        .map_err(|error| StorageError::sqlite("bootstrap download authority", error))?;
    let transcript_digest = digest(b"pod0:fresh-store:transcript-workflows:v1");
    transaction
        .execute(
            "INSERT INTO pod0_transcript_workflow_imports(singleton,source_generation,\
             source_fingerprint,backup_digest,backup_byte_count,row_count,state,staged_at_ms,\
             verified_at_ms,committed_at_ms) \
             VALUES(1,1,?1,?1,0,0,'authoritative',?2,?2,?2)",
            params![transcript_digest.as_slice(), observed_at_ms],
        )
        .map_err(|error| StorageError::sqlite("bootstrap transcript workflow import", error))?;
    transaction
        .execute(
            "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,core_revision,\
             committed_at_ms) VALUES('transcript_workflows','authoritative',1,1,?1)",
            [observed_at_ms],
        )
        .map_err(|error| StorageError::sqlite("bootstrap transcript workflow authority", error))?;
    transaction
        .execute(
            "UPDATE pod0_scheduled_agent_authority SET state='authoritative',\
             source_generation=1,core_revision=1,committed_at_ms=?1 \
             WHERE singleton=1 AND state='inactive'",
            [observed_at_ms],
        )
        .map_err(|error| StorageError::sqlite("bootstrap scheduled agent authority", error))?;
    Ok(())
}

fn derived_id(store_id: CommandId, label: &[u8]) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(b"pod0:fresh-authority-id:v1\0");
    hash.update(store_id.into_bytes());
    hash.update(label);
    CommandId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}

fn digest(value: &[u8]) -> [u8; 32] {
    Sha256::digest(value).into()
}

fn digest_hex(value: &[u8]) -> String {
    digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct BootstrapClock(i64);

impl MigrationClock for BootstrapClock {
    fn now_milliseconds(&self) -> i64 {
        self.0
    }
}
