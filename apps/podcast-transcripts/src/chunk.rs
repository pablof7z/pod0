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
mod tests {
    use super::*;
    use crate::types::{TranscriptEntry, TranscriptKind, TranscriptSource, TranscriptState};

    fn make_transcript(words_per_entry: &[u32]) -> Transcript {
        let mut entries: Vec<TranscriptEntry> = Vec::new();
        let mut cursor_secs = 0.0;
        let mut word_counter: u32 = 0;
        for (i, count) in words_per_entry.iter().enumerate() {
            let start = cursor_secs;
            let end = start + 1.0;
            let text: String = (0..*count)
                .map(|_| {
                    word_counter += 1;
                    format!("w{word_counter}")
                })
                .collect::<Vec<_>>()
                .join(" ");
            entries.push(TranscriptEntry {
                start_secs: start,
                end_secs: end,
                speaker: Some(format!("S{i}")),
                text,
                words: None,
            });
            cursor_secs = end;
        }
        Transcript {
            episode_id: "ep-1".into(),
            entries,
            source_url: "u".into(),
            kind: TranscriptKind::Vtt,
            status: TranscriptState::Ready {
                source: TranscriptSource::Publisher,
            },
            language: "en-US".into(),
        }
    }

    #[test]
    fn chunks_a_500_word_transcript_with_default_policy() {
        // 5 entries × 100 words = 500 words total. Default policy is
        // target=200 / overlap=20, so stride is 180. Chunks should be:
        // [0..200], [180..380], [360..500]. That's 3 chunks.
        let t = make_transcript(&[100, 100, 100, 100, 100]);
        let chunks = chunk_transcript(&t, ChunkPolicy::default());

        assert_eq!(chunks.len(), 3, "expected 3 chunks for a 500-word input");
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[0].word_count, 200);
        assert_eq!(chunks[1].chunk_index, 1);
        assert_eq!(chunks[1].word_count, 200);
        // Tail chunk: 500 - 360 = 140 words.
        assert_eq!(chunks[2].chunk_index, 2);
        assert_eq!(chunks[2].word_count, 140);

        // Overlap check: last 20 words of chunk 0 must equal the first 20
        // words of chunk 1.
        let c0_tail: Vec<&str> = chunks[0].text.split_whitespace().rev().take(20).collect();
        let c1_head: Vec<&str> = chunks[1].text.split_whitespace().take(20).collect();
        let c0_tail_in_order: Vec<&str> = c0_tail.into_iter().rev().collect();
        assert_eq!(c0_tail_in_order, c1_head, "overlap window mismatch");
    }

    #[test]
    fn empty_transcript_yields_no_chunks() {
        let t = make_transcript(&[]);
        assert!(chunk_transcript(&t, ChunkPolicy::default()).is_empty());
    }

    #[test]
    fn shorter_than_target_yields_one_chunk() {
        let t = make_transcript(&[50]);
        let chunks = chunk_transcript(&t, ChunkPolicy::default());
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].word_count, 50);
    }

    #[test]
    fn policy_clamps_overlap_below_target() {
        let policy = ChunkPolicy::new(10, 50);
        assert_eq!(policy.target_words, 10);
        assert_eq!(policy.overlap_words, 9);
    }

    #[test]
    fn timestamps_track_word_positions() {
        // Two entries, 100 words each, 1s apart. Default target=200 swallows
        // both into one chunk. Start should be entry 0's start (0.0), end
        // should be entry 1's end (2.0).
        let t = make_transcript(&[100, 100]);
        let chunks = chunk_transcript(&t, ChunkPolicy::default());
        assert_eq!(chunks.len(), 1);
        assert!((chunks[0].start_secs - 0.0).abs() < 1e-9);
        assert!((chunks[0].end_secs - 2.0).abs() < 1e-9);
    }

    #[test]
    fn deterministic_across_runs() {
        let t = make_transcript(&[100, 100, 100]);
        let a = chunk_transcript(&t, ChunkPolicy::default());
        let b = chunk_transcript(&t, ChunkPolicy::default());
        assert_eq!(a, b);
    }
}
