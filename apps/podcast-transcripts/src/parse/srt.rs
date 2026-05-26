//! SRT (SubRip) parser.
//!
//! Ported from the legacy `App/Sources/Transcript/Parsing/SRTParser.swift`.
//! SRT has no standard speaker convention, but publishers commonly prefix
//! cue text with one of:
//!
//! - `SPEAKER NAME: text`
//! - `[Speaker]: text`
//! - `>> Speaker: text`
//!
//! We recognise these shapes; anything else passes through unchanged.

use crate::parse::{normalise_newlines, parse_timestamp, ParseError};
use crate::types::{Transcript, TranscriptEntry, TranscriptKind, TranscriptSource, TranscriptState};

/// Parse an SRT document into a [`Transcript`] for `episode_id`.
pub fn parse_srt(
    source: &str,
    episode_id: impl Into<String>,
    source_url: impl Into<String>,
) -> Result<Transcript, ParseError> {
    let normalised = normalise_newlines(source);
    let trimmed = normalised.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }

    let mut entries: Vec<TranscriptEntry> = Vec::new();

    for block in trimmed.split("\n\n") {
        let lines: Vec<&str> = block
            .split('\n')
            .filter(|l| !l.is_empty())
            .collect();
        if lines.len() < 2 {
            continue;
        }

        // First line is usually a numeric index; skip it if the next line is
        // the timing line. Timing lines always contain "-->".
        let timing_idx = if lines[0].contains("-->") {
            0
        } else if lines.len() > 1 && lines[1].contains("-->") {
            1
        } else {
            continue;
        };

        let (start, end) = parse_timing(lines[timing_idx])?;
        let raw_text = lines[(timing_idx + 1)..].join(" ");
        let (speaker, clean_text) = extract_speaker(&raw_text);

        entries.push(TranscriptEntry {
            start_secs: start,
            end_secs: end,
            speaker,
            text: clean_text,
            words: None,
        });
    }

    entries.sort_by(|a, b| {
        a.start_secs
            .partial_cmp(&b.start_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(Transcript {
        episode_id: episode_id.into(),
        entries,
        source_url: source_url.into(),
        kind: TranscriptKind::Srt,
        status: TranscriptState::Ready {
            source: TranscriptSource::Publisher,
        },
        language: "en-US".to_string(),
    })
}

/// `HH:MM:SS,mmm --> HH:MM:SS,mmm`. Comma is the SRT decimal mark; the shared
/// timestamp parser also accepts dots because half the wild files use them.
fn parse_timing(line: &str) -> Result<(f64, f64), ParseError> {
    let parts: Vec<&str> = line.split(" --> ").collect();
    if parts.len() != 2 {
        return Err(ParseError::MalformedTiming(line.to_string()));
    }
    let start = parse_timestamp(parts[0])?;
    let end = parse_timestamp(parts[1])?;
    Ok((start, end))
}

/// Recognises the most common SRT speaker conventions.
fn extract_speaker(raw: &str) -> (Option<String>, String) {
    let mut text = raw.to_string();

    // Strip leading `>>` or `>` chevrons used by some captioners.
    while text.starts_with('>') {
        text.remove(0);
        let trimmed = text.trim_start_matches([' ', '>']).to_string();
        text = trimmed;
    }

    // Bracketed: `[Tim]: ...`
    if text.starts_with('[') {
        if let Some(close) = text.find(']') {
            // Search for the ":" after the closing bracket.
            if let Some(colon_rel) = text[close..].find(':') {
                let label = text[1..close].trim().to_string();
                let after_colon = text[close + colon_rel + 1..].trim().to_string();
                let speaker = if label.is_empty() { None } else { Some(label) };
                return (speaker, after_colon);
            }
        }
    }

    // Plain `Name: rest` — restrict to short plausible labels.
    if let Some(colon_idx) = text.find(':') {
        let label = text[..colon_idx].trim().to_string();
        if is_plausible_speaker_label(&label) {
            let rest = text[colon_idx + 1..].trim().to_string();
            return (Some(label), rest);
        }
    }

    (None, raw.to_string())
}

/// 1–4 word, ≤30 chars, contains at least one uppercase letter, no sentence
/// punctuation. Matches "Tim Ferriss", "PETER ATTIA", "Dr. Huberman" but not
/// "Yeah, well: I think" or "https://example.com".
fn is_plausible_speaker_label(s: &str) -> bool {
    if s.is_empty() || s.chars().count() > 30 {
        return false;
    }
    if s.contains("//") || s.contains(',') || s.contains('?') {
        return false;
    }
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.is_empty() || words.len() > 4 {
        return false;
    }
    for w in &words {
        let mut chars = w.chars();
        let first = match chars.next() {
            Some(c) => c,
            None => return false,
        };
        if !first.is_alphabetic() {
            return false;
        }
        // Letters, dots, hyphens, apostrophes — covers "Dr.", "O'Brien".
        for c in std::iter::once(first).chain(chars) {
            if !(c.is_alphabetic() || c == '.' || c == '-' || c == '\'') {
                return false;
            }
        }
    }
    s.chars().any(|c| c.is_uppercase())
}

#[cfg(test)]
#[path = "srt_tests.rs"]
mod tests;
