//! Transcript chunking.
//!
//! Slides a fixed word-count window across a [`Transcript`] producing
//! embedding-ready [`TranscriptChunk`]s. Each chunk records the
//! `(start_secs, end_secs)` covered by its source words so the search UI
//! can deep-link back to the episode timeline.
//!
//! The default policy (200-word window, 20-word overlap) is the M6 task
//! spec. The full speaker-snapping / token-budget chunker from the legacy
//! `ChunkBuilder` lands in M6.B alongside the vector capability — this
//! file is intentionally the simple, deterministic baseline.

use serde::{Deserialize, Serialize};

use crate::types::{Transcript, TranscriptChunk};

/// Chunking policy parameters.
///
/// `target_words` is the soft maximum per chunk; `overlap_words` is how
/// many trailing words from chunk `n` are re-included at the start of
/// chunk `n+1`. The chunker uses words as the unit (not tokens) so the
/// output is reproducible without a tokenizer dep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkPolicy {
    pub target_words: u32,
    pub overlap_words: u32,
}

impl Default for ChunkPolicy {
    fn default() -> Self {
        Self {
            target_words: 200,
            overlap_words: 20,
        }
    }
}

impl ChunkPolicy {
    /// Construct a custom policy.
    ///
    /// Both values are clamped to ≥1 word target / ≥0 overlap, and
    /// `overlap_words` is capped at `target_words - 1` so the window
    /// always advances at least one word per chunk.
    pub fn new(target_words: u32, overlap_words: u32) -> Self {
        let target = target_words.max(1);
        let overlap = overlap_words.min(target.saturating_sub(1));
        Self {
            target_words: target,
            overlap_words: overlap,
        }
    }
}

/// One word plus the entry it came from, used internally so we can
/// interpolate timestamps across entry boundaries.
#[derive(Debug, Clone)]
struct WordSlot {
    text: String,
    start_secs: f64,
    end_secs: f64,
}

/// Produce chunks for `transcript` under `policy`.
///
/// The chunker is deterministic — re-running on the same input always
/// produces the same output, so callers can use `(episode_id, chunk_index)`
/// as a stable primary key.
///
/// Behavior:
///
/// 1. Each [`TranscriptEntry::text`] is whitespace-split into words; each
///    word inherits the entry's `(start_secs, end_secs)` so chunk
///    boundaries get a tight time range.
/// 2. Words are accumulated until the chunk hits `policy.target_words`.
/// 3. The next chunk starts `target_words - overlap_words` words later
///    (i.e. an `overlap_words` suffix of the previous chunk is re-included).
/// 4. The final chunk may be shorter than the target.
pub fn chunk_transcript(transcript: &Transcript, policy: ChunkPolicy) -> Vec<TranscriptChunk> {
    let words = collect_words(transcript);
    if words.is_empty() {
        return Vec::new();
    }

    let target = policy.target_words as usize;
    let overlap = policy.overlap_words as usize;
    let stride = target.saturating_sub(overlap).max(1);

    let mut chunks: Vec<TranscriptChunk> = Vec::new();
    let mut cursor: usize = 0;
    let mut chunk_index: u32 = 0;

    while cursor < words.len() {
        let end = (cursor + target).min(words.len());
        let slice = &words[cursor..end];
        let text = slice
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let start_secs = slice.first().map(|w| w.start_secs).unwrap_or(0.0);
        let end_secs = slice.last().map(|w| w.end_secs).unwrap_or(start_secs);

        chunks.push(TranscriptChunk {
            episode_id: transcript.episode_id.clone(),
            chunk_index,
            start_secs,
            end_secs,
            text,
            word_count: slice.len() as u32,
        });
        chunk_index += 1;

        if end == words.len() {
            break;
        }
        cursor += stride;
    }

    chunks
}

/// Flatten the transcript into one `WordSlot` per whitespace-separated
/// token. Each word inherits the entry's time range.
fn collect_words(transcript: &Transcript) -> Vec<WordSlot> {
    let mut out: Vec<WordSlot> = Vec::new();
    for entry in &transcript.entries {
        // Prefer per-word timestamps when available — they give tighter
        // chunk time bounds. Fall back to the entry-wide range otherwise.
        if let Some(words) = &entry.words {
            for w in words {
                out.push(WordSlot {
                    text: w.text.clone(),
                    start_secs: w.start_secs,
                    end_secs: w.end_secs,
                });
            }
            continue;
        }
        for token in entry.text.split_whitespace() {
            out.push(WordSlot {
                text: token.to_string(),
                start_secs: entry.start_secs,
                end_secs: entry.end_secs,
            });
        }
    }
    out
}

#[cfg(test)]
#[path = "chunk_tests.rs"]
mod tests;
