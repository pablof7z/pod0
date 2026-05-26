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
