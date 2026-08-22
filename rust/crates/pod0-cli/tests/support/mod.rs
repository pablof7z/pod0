use std::path::Path;

use pod0_domain::CommandId;
use pod0_storage::{CURRENT_SCHEMA_VERSION, CoreStoreMigrator, MigrationClock};
use rusqlite::{Connection, params};

struct FixedClock;

impl MigrationClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        1_000
    }
}

/// A syntactically valid (but not cryptographically meaningful) sha256 hex
/// digest, satisfying every `pod0_*_imports.source_hash` `length(...)=64`
/// check constraint.
const FIXTURE_SOURCE_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// Bootstraps a fresh SQLite store at `path` that satisfies every authority
/// marker `Pod0Facade::open` requires — authoritative `listening`/`notes`/
/// `transcripts`/`clips` cutovers (each backed by the minimal import + state
/// singleton row their authority/read checks require) plus an authoritative
/// `pod0_chapter_state` row — without ever calling the disabled
/// `Pod0Facade::create`.
pub fn bootstrap_authoritative_store(path: &Path) {
    let backup_path = path.with_file_name(format!(
        "{}.backup",
        path.file_name()
            .expect("store path must have a file name")
            .to_string_lossy()
    ));
    CoreStoreMigrator::new(FixedClock)
        .migrate(
            path,
            CURRENT_SCHEMA_VERSION,
            &backup_path,
            CommandId::from_bytes([1; 16]),
        )
        .unwrap();

    let connection = Connection::open(path).unwrap();

    connection
        .execute(
            "INSERT INTO pod0_listening_imports(import_id,source_kind,source_hash,\
             source_generation,podcast_count,subscription_count,episode_count,\
             backup_byte_count,target_revision,state,verified_at_ms) \
             VALUES(zeroblob(16),1,?1,0,0,0,0,1,1,'verified',1000)",
            params![FIXTURE_SOURCE_HASH],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO pod0_playback_state(singleton,active_episode_id,\
             playback_rate_permille,sleep_mode_code,sleep_duration_ms,sleep_wire_code,\
             auto_mark_played_at_natural_end,auto_play_next,state_revision,\
             source_import_id,active_segment_start_ms,active_segment_end_ms,\
             active_segment_label) \
             VALUES(1,NULL,1000,255,NULL,0,0,0,1,zeroblob(16),NULL,NULL,NULL)",
            [],
        )
        .unwrap();

    connection
        .execute(
            "INSERT INTO pod0_note_imports(import_id,source_kind,source_hash,\
             source_generation,note_count,backup_byte_count,target_revision,state,\
             verified_at_ms) VALUES(zeroblob(16),1,?1,0,0,1,1,'verified',1000)",
            params![FIXTURE_SOURCE_HASH],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO pod0_note_state(singleton,collection_revision,source_import_id) \
             VALUES(1,1,zeroblob(16))",
            [],
        )
        .unwrap();

    connection
        .execute(
            "INSERT INTO pod0_clip_imports(import_id,source_kind,source_hash,\
             source_generation,clip_count,backup_byte_count,target_revision,state,\
             verified_at_ms) VALUES(zeroblob(16),1,?1,0,0,1,1,'verified',1000)",
            params![FIXTURE_SOURCE_HASH],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO pod0_clip_state(singleton,collection_revision,source_import_id) \
             VALUES(1,1,zeroblob(16))",
            [],
        )
        .unwrap();

    for domain in ["listening", "notes", "transcripts", "clips"] {
        connection
            .execute(
                "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,\
                 core_revision,committed_at_ms) VALUES(?1,'authoritative',0,1,1000)",
                params![domain],
            )
            .unwrap();
    }

    let import_id = CommandId::from_bytes([2; 16]);
    connection
        .execute(
            "INSERT INTO pod0_chapter_imports(import_id,source_kind,source_identity,\
             source_generation,source_byte_count,source_database_digest,\
             source_selection_digest,command_fingerprint,evidence_count,artifact_count,\
             selected_count,blocked_count,chapter_count,ad_span_count,target_revision,state,\
             backup_database_digest,backup_database_byte_count,backup_file_count,\
             backup_file_byte_count,staged_at_ms,verified_at_ms,imported_at_ms) \
             VALUES(?1,'artifact_sqlite_v1',zeroblob(32),0,0,zeroblob(32),zeroblob(32),\
             zeroblob(32),0,0,0,0,0,0,1,'imported',zeroblob(32),0,0,0,1000,1000,1000)",
            [import_id.into_bytes().as_slice()],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE pod0_chapter_state SET authority_active=1,authority_import_id=?1 \
             WHERE singleton=1",
            [import_id.into_bytes().as_slice()],
        )
        .unwrap();
}
