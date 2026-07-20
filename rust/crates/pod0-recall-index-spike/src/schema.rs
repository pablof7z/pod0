use std::sync::OnceLock;

use rusqlite::ffi::{SQLITE_OK, sqlite3_auto_extension};

use crate::{RecallIndexError, RecallIndexSpike};

static SQLITE_VEC_REGISTRATION: OnceLock<Result<(), i32>> = OnceLock::new();

pub(crate) fn register_sqlite_vec() -> Result<(), RecallIndexError> {
    let result = SQLITE_VEC_REGISTRATION.get_or_init(|| {
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

impl RecallIndexSpike {
    pub(crate) fn initialize_schema(&self) -> Result<(), RecallIndexError> {
        self.connection.execute_batch(&format!(
            "PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS recall_spike_meta(
               span_id TEXT PRIMARY KEY,
               generation_id TEXT NOT NULL,
               episode_id TEXT NOT NULL,
               podcast_id TEXT NOT NULL,
               text TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS recall_spike_episode
               ON recall_spike_meta(episode_id);
             CREATE VIRTUAL TABLE IF NOT EXISTS recall_spike_vec USING vec0(
               span_id TEXT PRIMARY KEY,
               episode_id TEXT PARTITION KEY,
               podcast_id TEXT PARTITION KEY,
               embedding FLOAT[{}] distance_metric=cosine,
               chunk_size=128
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS recall_spike_fts USING fts5(
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
}
