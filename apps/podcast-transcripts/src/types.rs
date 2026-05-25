//! Transcript domain types for the parsing + chunking layer.
//!
//! `Transcript` is the lossless in-memory shape produced by every parser
//! (publisher VTT/SRT/JSON, on-device STT, paid cloud STT). `TranscriptChunk`
//! is the embedding-ready window emitted by [`crate::chunk::chunk_transcript`]
//! and consumed by `podcast-knowledge` for RAG indexing.
//!
//! Re-uses `TranscriptKind` and the `TranscriptState` enum from
//! `podcast-core::types::transcript`. The task spec calls the latter
//! `TranscriptStatus`; we re-export under that name as well for spec parity
//! while keeping a single canonical type in `podcast-core`.

use serde::{Deserialize, Serialize};

pub use podcast_core::{TranscriptKind, TranscriptSource, TranscriptState};

/// Spec-name alias for [`podcast_core::TranscriptState`].
///
/// The M6 task spec uses `TranscriptStatus`; the canonical domain type lives
/// in `podcast-core` as `TranscriptState`. Keeping a single source of truth
/// avoids drift — this alias is purely a naming bridge.
pub type TranscriptStatus = TranscriptState;

/// A single time-stamped utterance.
///
/// Times are in floating-point seconds from episode start. Word-level
/// timestamps are populated when the source provides them (Podcasting 2.0
/// JSON with `words` arrays, ElevenLabs Scribe); otherwise [`None`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub start_secs: f64,
    pub end_secs: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    pub text: String,
    /// Optional per-word timestamps. Preserved from the legacy Swift parsers
    /// so karaoke highlighting and word-snap clip boundaries keep working.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<TranscriptWord>>,
}

/// Word-level timestamp inside an entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptWord {
    pub start_secs: f64,
    pub end_secs: f64,
    pub text: String,
}

/// In-memory representation of one episode's transcript.
///
/// `source_url` is whatever the ingestor pulled the bytes from (publisher
/// `<podcast:transcript>` URL, an STT job result URL, or a synthesised
/// `data:` URL for on-device runs). `kind` records the on-the-wire format
/// before parsing; downstream code uses it for diagnostics and re-fetch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    pub episode_id: String,
    pub entries: Vec<TranscriptEntry>,
    pub source_url: String,
    pub kind: TranscriptKind,
    pub status: TranscriptStatus,
    /// BCP-47 language code (e.g. "en-US"). Defaults to `"en-US"` when the
    /// source doesn't carry one.
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_language() -> String {
    "en-US".to_string()
}

impl Transcript {
    /// New transcript with `TranscriptState::Ready` status.
    pub fn ready(
        episode_id: impl Into<String>,
        entries: Vec<TranscriptEntry>,
        source_url: impl Into<String>,
        kind: TranscriptKind,
        source: TranscriptSource,
    ) -> Self {
        Self {
            episode_id: episode_id.into(),
            entries,
            source_url: source_url.into(),
            kind,
            status: TranscriptState::Ready { source },
            language: default_language(),
        }
    }
}

/// One embedding-ready window over a [`Transcript`].
///
/// `chunk_index` is the zero-based ordinal within the episode and is stable
/// across re-chunks of the same input (the chunker is deterministic on
/// input + policy). `start_secs` / `end_secs` are derived from the entries
/// that contributed; `word_count` is the chunk's contribution to the
/// embedding budget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptChunk {
    pub episode_id: String,
    pub chunk_index: u32,
    pub start_secs: f64,
    pub end_secs: f64,
    pub text: String,
    pub word_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_entry_round_trip() {
        let entry = TranscriptEntry {
            start_secs: 1.5,
            end_secs: 4.25,
            speaker: Some("Host".to_string()),
            text: "Hello world".to_string(),
            words: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: TranscriptEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, back);
    }

    #[test]
    fn transcript_ready_helper() {
        let transcript = Transcript::ready(
            "ep-1",
            vec![],
            "https://example.com/t.vtt",
            TranscriptKind::Vtt,
            TranscriptSource::Publisher,
        );
        assert!(matches!(
            transcript.status,
            TranscriptState::Ready {
                source: TranscriptSource::Publisher
            }
        ));
        assert_eq!(transcript.language, "en-US");
    }

    #[test]
    fn transcript_chunk_round_trip() {
        let chunk = TranscriptChunk {
            episode_id: "ep-1".into(),
            chunk_index: 3,
            start_secs: 12.0,
            end_secs: 60.0,
            text: "some text".into(),
            word_count: 2,
        };
        let json = serde_json::to_string(&chunk).unwrap();
        let back: TranscriptChunk = serde_json::from_str(&json).unwrap();
        assert_eq!(chunk, back);
    }
}
