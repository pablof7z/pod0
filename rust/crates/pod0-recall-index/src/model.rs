use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pod0_application::RecallEmbeddingVector;
use pod0_domain::{EpisodeId, EvidenceGenerationId, EvidenceSpanId, PodcastId};
use rusqlite::Connection;

pub const RECALL_INDEX_SCHEMA_VERSION: u32 = 1;
pub const RECALL_INDEX_DIMENSIONS: usize = 1_024;
pub const MAX_RECALL_EMBEDDING_BATCH: usize = 16;

#[must_use]
pub fn recall_index_path_for_core_store(core_store: &Path) -> PathBuf {
    let mut value = std::ffi::OsString::from(core_store.as_os_str());
    value.push(".recall-index.sqlite");
    PathBuf::from(value)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecallIndexSpan {
    pub span_id: EvidenceSpanId,
    pub generation_id: EvidenceGenerationId,
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecallEmbeddingRequest {
    pub span_id: EvidenceSpanId,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecallSpanEmbedding {
    pub span_id: EvidenceSpanId,
    pub embedding: RecallEmbeddingVector,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecallIndexPlan {
    Ready { indexed_span_count: u32 },
    NeedsEmbeddings { spans: Vec<RecallEmbeddingRequest> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecallIndexCandidate {
    pub episode_id: EpisodeId,
    pub generation_id: EvidenceGenerationId,
    pub span_id: EvidenceSpanId,
    pub vector_rank: Option<u16>,
    pub lexical_rank: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecallIndexCutoverReceipt {
    pub schema_version: u32,
    pub removed_legacy_file_count: u8,
}

#[derive(Clone, Default)]
pub struct RecallCancellation {
    cancelled: Arc<AtomicBool>,
    interrupt: Option<Arc<rusqlite::InterruptHandle>>,
}

impl RecallCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(interrupt) = &self.interrupt {
            interrupt.interrupt();
        }
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Debug)]
pub enum RecallIndexError {
    Cancelled,
    InvalidInput(&'static str),
    IncompatibleSchema,
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
}

impl fmt::Display for RecallIndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("recall index operation cancelled"),
            Self::InvalidInput(detail) => formatter.write_str(detail),
            Self::IncompatibleSchema => formatter.write_str("recall index schema is incompatible"),
            Self::Sqlite(_) => formatter.write_str("recall index storage is unavailable"),
            Self::Io(_) => formatter.write_str("recall index artifact is unavailable"),
        }
    }
}

impl std::error::Error for RecallIndexError {}

impl From<rusqlite::Error> for RecallIndexError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

impl From<std::io::Error> for RecallIndexError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub struct RecallIndex {
    pub(crate) connection: Connection,
    pub(crate) dimensions: usize,
}

impl RecallIndex {
    pub fn open(path: &Path, dimensions: usize) -> Result<Self, RecallIndexError> {
        crate::schema::register_sqlite_vec()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::migration::validate_disposable_artifacts(path)?;
        let result = Self::open_connection(Connection::open(path)?, dimensions);
        match result {
            Err(error) if error.is_recoverable_corruption() => {
                crate::migration::remove_disposable_artifacts(path)?;
                Self::open_connection(Connection::open(path)?, dimensions)
            }
            result => result,
        }
    }

    pub fn in_memory(dimensions: usize) -> Result<Self, RecallIndexError> {
        crate::schema::register_sqlite_vec()?;
        Self::open_connection(Connection::open_in_memory()?, dimensions)
    }

    fn open_connection(
        connection: Connection,
        dimensions: usize,
    ) -> Result<Self, RecallIndexError> {
        if !(1..=pod0_application::MAX_RECALL_EMBEDDING_DIMENSIONS).contains(&dimensions) {
            return Err(RecallIndexError::InvalidInput(
                "recall index dimensions are outside the bounded contract",
            ));
        }
        let value = Self {
            connection,
            dimensions,
        };
        value.initialize_schema()?;
        Ok(value)
    }

    #[must_use]
    pub fn cancellation(&self) -> RecallCancellation {
        RecallCancellation {
            cancelled: Arc::default(),
            interrupt: Some(Arc::new(self.connection.get_interrupt_handle())),
        }
    }

    pub fn sqlite_vec_version(&self) -> Result<String, RecallIndexError> {
        self.connection
            .query_row("SELECT vec_version()", [], |row| row.get(0))
            .map_err(Into::into)
    }
}

impl RecallIndexError {
    fn is_recoverable_corruption(&self) -> bool {
        matches!(
            self,
            Self::Sqlite(rusqlite::Error::SqliteFailure(error, _))
                if matches!(
                    error.code,
                    rusqlite::ffi::ErrorCode::DatabaseCorrupt
                        | rusqlite::ffi::ErrorCode::NotADatabase
                )
        )
    }
}
