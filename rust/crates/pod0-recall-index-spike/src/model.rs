use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pod0_application::RecallEmbeddingVector;
use pod0_domain::{EpisodeId, EvidenceGenerationId, EvidenceSpanId, PodcastId};
use rusqlite::Connection;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecallIndexSpan {
    pub span_id: EvidenceSpanId,
    pub generation_id: EvidenceGenerationId,
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub text: String,
    pub embedding: RecallEmbeddingVector,
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

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Debug)]
pub enum RecallIndexError {
    Cancelled,
    InvalidInput(&'static str),
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
}

impl fmt::Display for RecallIndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("recall index operation cancelled"),
            Self::InvalidInput(detail) => formatter.write_str(detail),
            Self::Sqlite(error) => write!(formatter, "recall index sqlite error: {error}"),
            Self::Io(error) => write!(formatter, "recall index io error: {error}"),
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

pub struct RecallIndexSpike {
    pub(crate) connection: Connection,
    pub(crate) dimensions: usize,
}

impl RecallIndexSpike {
    pub fn open(path: &Path, dimensions: usize) -> Result<Self, RecallIndexError> {
        crate::schema::register_sqlite_vec()?;
        Self::open_connection(Connection::open(path)?, dimensions)
    }

    pub fn in_memory(dimensions: usize) -> Result<Self, RecallIndexError> {
        crate::schema::register_sqlite_vec()?;
        Self::open_connection(Connection::open_in_memory()?, dimensions)
    }

    fn open_connection(
        connection: Connection,
        dimensions: usize,
    ) -> Result<Self, RecallIndexError> {
        if !(1..=4_096).contains(&dimensions) {
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

    pub fn interrupt_handle(&self) -> rusqlite::InterruptHandle {
        self.connection.get_interrupt_handle()
    }

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
