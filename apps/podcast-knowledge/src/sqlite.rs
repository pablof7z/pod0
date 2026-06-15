//! SQLite-backed durable sidecar for [`crate::KnowledgeStore`].
//!
//! `KnowledgeSqliteStore` is a **write-through** companion: the in-memory
//! [`crate::KnowledgeStore`] stays authoritative; this store is only consulted
//! at cold start (`load_all`) and updated on every `upsert` / `delete_episode`.
//!
//! Schema version is tracked in a single-row `schema_meta` table.  Future
//! slices add `ALTER TABLE` steps inside `migrate()`.
//!
//! Corrupt-file quarantine: if `Connection::open` or `migrate()` fails the
//! file is renamed to `<path>.corrupt-<unix_ts>` and an in-memory (`:memory:`)
//! connection is returned as a no-op fallback — chunks will rebuild on
//! re-ingest.  Errors in `upsert` / `delete_episode` are silently ignored (D6);
//! the in-memory store is always authoritative.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::types::{EmbeddingVector, KnowledgeChunk};
use podcast_transcripts::TranscriptChunk;

const CURRENT_SCHEMA_VERSION: i64 = 1;

/// SQLite-backed durable sidecar for the knowledge chunk store.
pub struct KnowledgeSqliteStore {
    conn: Connection,
}

impl KnowledgeSqliteStore {
    /// Open (or create) the SQLite file at `path`.
    ///
    /// Runs schema migration on open.  If the file is corrupt or the migration
    /// fails, the file is quarantined (renamed to `<path>.corrupt-<ts>`) and
    /// an in-memory fallback is returned — callers need not handle the
    /// difference since `load_all` will just return an empty vec.
    pub fn open(path: &Path) -> Self {
        match Self::try_open(path) {
            Ok(store) => store,
            Err(e) => {
                // Quarantine the corrupt file.
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let quarantine = format!("{}.corrupt-{}", path.display(), ts);
                if let Err(rename_err) = std::fs::rename(path, &quarantine) {
                    eprintln!(
                        "[knowledge-sqlite] quarantine rename failed: {rename_err}; \
                         original open error: {e}"
                    );
                } else {
                    eprintln!(
                        "[knowledge-sqlite] corrupt DB quarantined to {quarantine}: {e}"
                    );
                }
                // Fall back to an in-memory store — chunks will rebuild on re-ingest.
                let conn = Connection::open_in_memory()
                    .expect("in-memory SQLite must always succeed");
                let store = Self { conn };
                // Migrate the in-memory DB so the schema is ready for writes.
                let _ = store.migrate();
                store
            }
        }
    }

    fn try_open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        // WAL mode for better write concurrency and crash safety.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    /// Run schema migrations.  Idempotent — safe to call multiple times.
    fn migrate(&self) -> Result<(), rusqlite::Error> {
        // Ensure schema_meta exists first so we can read the version.
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_meta (version INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS chunks (
                 episode_id   TEXT    NOT NULL,
                 chunk_index  INTEGER NOT NULL,
                 start_secs   REAL    NOT NULL,
                 end_secs     REAL    NOT NULL,
                 word_count   INTEGER NOT NULL,
                 text         TEXT    NOT NULL,
                 embedding    BLOB,
                 embedding_dim INTEGER,
                 PRIMARY KEY (episode_id, chunk_index)
             );",
        )?;

        let version: Option<i64> = self
            .conn
            .query_row(
                "SELECT version FROM schema_meta LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;

        match version {
            None => {
                // Fresh database — insert initial version marker.
                self.conn.execute(
                    "INSERT INTO schema_meta (version) VALUES (?1)",
                    params![CURRENT_SCHEMA_VERSION],
                )?;
            }
            Some(v) if v == CURRENT_SCHEMA_VERSION => {
                // Already at current version — nothing to do.
            }
            Some(v) => {
                // Future slices add ALTER TABLE migration steps here,
                // chaining v1→v2→… with explicit UPDATE schema_meta calls.
                eprintln!("[knowledge-sqlite] unknown schema version {v}; treating as current");
            }
        }
        Ok(())
    }

    /// Write-through upsert.  Failures are returned to the caller; callers
    /// are expected to swallow them (the in-memory store is authoritative).
    pub fn upsert(&self, chunk: &KnowledgeChunk) -> Result<(), rusqlite::Error> {
        let (embedding_bytes, embedding_dim): (Option<Vec<u8>>, Option<i64>) =
            match &chunk.embedding {
                Some(ev) => {
                    let bytes = f32_slice_to_bytes(ev.as_slice());
                    let dim = ev.dim() as i64;
                    (Some(bytes), Some(dim))
                }
                None => (None, None),
            };

        self.conn.execute(
            "INSERT OR REPLACE INTO chunks
             (episode_id, chunk_index, start_secs, end_secs, word_count, text,
              embedding, embedding_dim)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                chunk.chunk.episode_id,
                chunk.chunk.chunk_index as i64,
                chunk.chunk.start_secs,
                chunk.chunk.end_secs,
                chunk.chunk.word_count as i64,
                chunk.chunk.text,
                embedding_bytes,
                embedding_dim,
            ],
        )?;
        Ok(())
    }

    /// Delete all chunks for an episode.  Write-through from the in-memory
    /// store.  Failures are returned; callers are expected to swallow them.
    pub fn delete_episode(&self, episode_id: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "DELETE FROM chunks WHERE episode_id = ?1",
            params![episode_id],
        )?;
        Ok(())
    }

    /// Load all chunks from SQLite into memory.  Called at cold start
    /// (`set_data_dir`) to seed the in-memory `KnowledgeStore`.
    pub fn load_all(&self) -> Vec<KnowledgeChunk> {
        let mut stmt = match self.conn.prepare(
            "SELECT episode_id, chunk_index, start_secs, end_secs, word_count, text,
                    embedding, embedding_dim
             FROM chunks",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[knowledge-sqlite] load_all prepare failed: {e}");
                return Vec::new();
            }
        };

        let rows = stmt.query_map([], |row| {
            let episode_id: String = row.get(0)?;
            let chunk_index: i64 = row.get(1)?;
            let start_secs: f64 = row.get(2)?;
            let end_secs: f64 = row.get(3)?;
            let word_count: i64 = row.get(4)?;
            let text: String = row.get(5)?;
            let embedding_bytes: Option<Vec<u8>> = row.get(6)?;
            let embedding_dim: Option<i64> = row.get(7)?;

            let embedding = match (embedding_bytes, embedding_dim) {
                (Some(bytes), Some(dim)) => {
                    let vec = bytes_to_f32_vec(&bytes, dim as usize);
                    Some(EmbeddingVector::new(vec))
                }
                _ => None,
            };

            Ok(KnowledgeChunk {
                chunk: TranscriptChunk {
                    episode_id,
                    chunk_index: chunk_index as u32,
                    start_secs,
                    end_secs,
                    text,
                    word_count: word_count as u32,
                },
                embedding,
            })
        });

        match rows {
            Ok(iter) => iter
                .filter_map(|r| {
                    r.map_err(|e| {
                        eprintln!("[knowledge-sqlite] row decode error: {e}");
                        e
                    })
                    .ok()
                })
                .collect(),
            Err(e) => {
                eprintln!("[knowledge-sqlite] load_all query failed: {e}");
                Vec::new()
            }
        }
    }
}

// ── Embedding serialization helpers ──────────────────────────────────────────

/// Serialize a `&[f32]` to little-endian bytes.
fn f32_slice_to_bytes(values: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for &v in values {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

/// Deserialize little-endian bytes back to `Vec<f32>`.
///
/// `expected_dim` is the stored `embedding_dim` column; if the byte length
/// does not match, a truncated / zero-padded vec is returned defensively
/// rather than panicking.
fn bytes_to_f32_vec(bytes: &[u8], expected_dim: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(expected_dim);
    for chunk in bytes.chunks(4) {
        if chunk.len() == 4 {
            let arr: [u8; 4] = [chunk[0], chunk[1], chunk[2], chunk[3]];
            out.push(f32::from_le_bytes(arr));
        }
    }
    // Zero-pad if byte count was short (defensive).
    while out.len() < expected_dim {
        out.push(0.0);
    }
    out
}
