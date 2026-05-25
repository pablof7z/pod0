//! `podcast-transcripts` — transcript parsing, chunking, and STT ingestion
//! action types.
//!
//! Pure parsing and decision logic. HTTP fetching, STT job submission, and
//! provider callbacks all live in `nmp.stt.capability` (M5); this crate
//! accepts bytes/strings and produces values from `podcast-core` / its own
//! [`types`] module.
//!
//! Public surface:
//!
//! - [`parse_vtt`], [`parse_srt`], [`parse_podcasting_json`] — format-specific
//!   parsers that return a [`Transcript`].
//! - [`chunk_transcript`] — word-window chunker emitting [`TranscriptChunk`]s
//!   for downstream embedding via `podcast-knowledge`.
//! - [`actions`] — ingestion action types dispatched by the kernel.

pub mod actions;
pub mod chunk;
pub mod parse;
pub mod types;

pub use actions::{IngestTranscript, OverrideProvider, RetryTranscript};
pub use chunk::{chunk_transcript, ChunkPolicy};
pub use parse::{parse_podcasting_json, parse_srt, parse_vtt, ParseError};
pub use types::{
    Transcript, TranscriptChunk, TranscriptEntry, TranscriptKind, TranscriptSource,
    TranscriptState, TranscriptStatus, TranscriptWord,
};
