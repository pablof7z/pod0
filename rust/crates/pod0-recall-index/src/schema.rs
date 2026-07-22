#![allow(unsafe_code)]

use std::sync::OnceLock;

use rusqlite::ffi::{SQLITE_OK, sqlite3_auto_extension};
use std::path::Path;

use rusqlite::{OptionalExtension, params};

use crate::{RECALL_INDEX_SCHEMA_VERSION, RecallIndex, RecallIndexError};

static SQLITE_VEC_REGISTRATION: OnceLock<Result<(), i32>> = OnceLock::new();

pub(crate) fn register_sqlite_vec() -> Result<(), RecallIndexError> {
    let result = SQLITE_VEC_REGISTRATION.get_or_init(|| {
        // sqlite-vec exposes the standard SQLite extension ABI. The pointer
        // conversion is isolated here and pinned by the exact crate version.
        let entrypoint = unsafe {
            std::mem::transmute::<
                *const (),
                unsafe extern "C" fn(
                    *mut rusqlite::ffi::sqlite3,
                    *mut *mut std::ffi::c_char,
                    *const rusqlite::ffi::sqlite3_api_routines,
                ) -> std::ffi::c_int,
            >(sqlite_vec::sqlite3_vec_init as *const ())
        };
        let code = unsafe { sqlite3_auto_extension(Some(entrypoint)) };
        (code == SQLITE_OK).then_some(()).ok_or(code)
    });
    match *result {
        Ok(()) => Ok(()),
        Err(_) => Err(RecallIndexError::InvalidInput(
            "sqlite-vec registration failed",
        )),
    }
}

impl RecallIndex {
    pub(crate) fn initialize_schema(&self) -> Result<(), RecallIndexError> {
        self.connection.execute_batch(
            "PRAGMA foreign_keys=ON;
             PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS pod0_recall_index_metadata(
               singleton INTEGER PRIMARY KEY CHECK(singleton=1),
               schema_version INTEGER NOT NULL,
               dimensions INTEGER NOT NULL,
               owner TEXT NOT NULL CHECK(owner='rust'),
               legacy_cutover_committed INTEGER NOT NULL DEFAULT 0,
               embedding_space_digest BLOB
                 CHECK(embedding_space_digest IS NULL OR length(embedding_space_digest)=32)
             );",
        )?;
        let existing = self
            .connection
            .query_row(
                "SELECT schema_version,dimensions,owner
                 FROM pod0_recall_index_metadata WHERE singleton=1",
                [],
                |row| {
                    Ok((
                        row.get::<_, u32>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        if let Some((version, dimensions, owner)) = existing {
            if version != RECALL_INDEX_SCHEMA_VERSION
                || usize::try_from(dimensions).ok() != Some(self.dimensions)
                || owner != "rust"
            {
                return Err(RecallIndexError::IncompatibleSchema);
            }
        } else {
            self.connection.execute(
                "INSERT INTO pod0_recall_index_metadata(
                   singleton,schema_version,dimensions,owner
                 ) VALUES(1,?1,?2,'rust')",
                params![
                    RECALL_INDEX_SCHEMA_VERSION,
                    u32::try_from(self.dimensions).expect("bounded dimensions")
                ],
            )?;
        }
        self.connection
            .pragma_update(None, "user_version", RECALL_INDEX_SCHEMA_VERSION)?;
        self.initialize_cache_schema()?;
        self.initialize_execution_schema()?;
        Ok(())
    }

    fn initialize_cache_schema(&self) -> Result<(), RecallIndexError> {
        self.connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS pod0_recall_embedding_cache_v1(
               span_id TEXT NOT NULL,
               generation_id TEXT NOT NULL,
               episode_id TEXT NOT NULL,
               podcast_id TEXT NOT NULL,
               text_digest BLOB NOT NULL CHECK(length(text_digest)=32),
               dimensions INTEGER NOT NULL,
               embedding BLOB NOT NULL,
               PRIMARY KEY(span_id,generation_id)
             );
             CREATE INDEX IF NOT EXISTS pod0_recall_cache_episode_v1
               ON pod0_recall_embedding_cache_v1(episode_id);",
        )?;
        Ok(())
    }

    pub(crate) fn initialize_execution_schema(&self) -> Result<(), RecallIndexError> {
        self.connection.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS pod0_recall_generations_v1(
               episode_id TEXT PRIMARY KEY,
               generation_id TEXT NOT NULL,
               podcast_id TEXT NOT NULL,
               span_count INTEGER NOT NULL CHECK(span_count>0)
             );
             CREATE INDEX IF NOT EXISTS pod0_recall_generation_podcast_v1
               ON pod0_recall_generations_v1(podcast_id);
             CREATE TABLE IF NOT EXISTS pod0_recall_meta_v1(
               span_id TEXT PRIMARY KEY,
               generation_id TEXT NOT NULL,
               episode_id TEXT NOT NULL,
               podcast_id TEXT NOT NULL,
               text TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS pod0_recall_episode_v1
               ON pod0_recall_meta_v1(episode_id);
             CREATE VIRTUAL TABLE IF NOT EXISTS pod0_recall_vec_v1 USING vec0(
               span_id TEXT PRIMARY KEY,
               episode_id TEXT PARTITION KEY,
               podcast_id TEXT PARTITION KEY,
               embedding FLOAT[{}] distance_metric=cosine,
               chunk_size=128
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS pod0_recall_fts_v1 USING fts5(
               span_id UNINDEXED,
               episode_id UNINDEXED,
               podcast_id UNINDEXED,
               text,
               tokenize='porter'
             );",
            self.dimensions
        ))?;
        Ok(())
    }

    pub fn reset_execution_tables(&mut self) -> Result<(), RecallIndexError> {
        self.connection.execute_batch(
            "BEGIN IMMEDIATE;
             DROP TABLE IF EXISTS pod0_recall_vec_v1;
             DROP TABLE IF EXISTS pod0_recall_fts_v1;
             DROP TABLE IF EXISTS pod0_recall_meta_v1;
             DROP TABLE IF EXISTS pod0_recall_generations_v1;
             COMMIT;",
        )?;
        self.initialize_execution_schema()
    }
}

pub(crate) fn stored_schema_is_older(path: &Path) -> Result<bool, RecallIndexError> {
    let connection = rusqlite::Connection::open(path)?;
    let stored = connection
        .query_row(
            "SELECT schema_version FROM pod0_recall_index_metadata WHERE singleton=1",
            [],
            |row| row.get::<_, u32>(0),
        )
        .optional()?;
    Ok(stored.is_some_and(|version| version < RECALL_INDEX_SCHEMA_VERSION))
}
