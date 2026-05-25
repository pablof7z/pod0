//! WebVTT parser.
//!
//! Ported from the legacy `App/Sources/Transcript/Parsing/VTTParser.swift`.
//! Recognises:
//!
//! - Standard cue header `WEBVTT`, optional `NOTE` / `STYLE` / `REGION`
//!   blocks (skipped).
//! - Optional cue identifier line (skipped).
//! - Timestamp line `HH:MM:SS.mmm --> HH:MM:SS.mmm` and the `MM:SS.mmm`
//!   variant used by some encoders.
//! - Speaker tags `<v Tim Ferriss>...</v>` per Podcasting 2.0 convention.
//!
//! Deliberately drops cue settings (alignment / position), styling and
//! region blocks — podcast transcripts use ~none of this and stripping
//! keeps the parser dependency-free.

use crate::parse::{normalise_newlines, parse_timestamp, ParseError};
use crate::types::{Transcript, TranscriptEntry, TranscriptKind, TranscriptSource, TranscriptState};

/// Parse a WebVTT document into a [`Transcript`] for `episode_id`.
///
/// `source_url` is recorded on the returned transcript so callers can trace
/// re-fetches. Language defaults to the empty form; callers that know the
/// language should set it on the returned value.
pub fn parse_vtt(
    source: &str,
    episode_id: impl Into<String>,
    source_url: impl Into<String>,
) -> Result<Transcript, ParseError> {
    let normalised = normalise_newlines(source);
    let mut blocks = normalised.split("\n\n");

    let header = blocks.next().ok_or(ParseError::MissingHeader)?;
    if !header.starts_with("WEBVTT") {
        return Err(ParseError::MissingHeader);
    }

    let mut entries: Vec<TranscriptEntry> = Vec::new();

    for block in blocks {
        let lines: Vec<&str> = block.split('\n').collect();
        if lines.is_empty() {
            continue;
        }

        // Skip NOTE / STYLE / REGION blocks.
        let head = lines[0];
        if head.starts_with("NOTE") || head.starts_with("STYLE") || head.starts_with("REGION") {
            continue;
        }

        // The first line that contains "-->" is the timing line; preceding
        // lines (cue identifier) are skipped.
        let Some(timing_idx) = lines.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        let (start, end) = parse_timing(lines[timing_idx])?;

        let raw_text: String = lines[(timing_idx + 1)..]
            .iter()
            .filter(|l| !l.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ");

        let (speaker, clean_text) = extract_speaker(&raw_text);
        entries.push(TranscriptEntry {
            start_secs: start,
            end_secs: end,
            speaker,
            text: clean_text,
            words: None,
        });
    }

    // Defensive sort. Most files are ordered, but cheap insurance.
    entries.sort_by(|a, b| {
        a.start_secs
            .partial_cmp(&b.start_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(Transcript {
        episode_id: episode_id.into(),
        entries,
        source_url: source_url.into(),
        kind: TranscriptKind::Vtt,
        status: TranscriptState::Ready {
            source: TranscriptSource::Publisher,
        },
        language: "en-US".to_string(),
    })
}

/// Parse a single VTT timing line into `(start_secs, end_secs)`.
fn parse_timing(line: &str) -> Result<(f64, f64), ParseError> {
    let trimmed = line.trim();
    let parts: Vec<&str> = trimmed.split(" --> ").collect();
    if parts.len() < 2 {
        return Err(ParseError::MalformedTiming(line.to_string()));
    }
    let start = parse_timestamp(parts[0])?;
    // Right side may carry cue settings: "00:01:00.000 align:start".
    let right_raw = parts[1].split_whitespace().next().unwrap_or(parts[1]);
    let end = parse_timestamp(right_raw)?;
    Ok((start, end))
}

/// `<v Speaker Name>text...` → ("Speaker Name", "text...").
/// Falls back to plain text when no `<v>` tag is present. Strips any
/// remaining VTT tags (`<c>`, `<i>`, `<00:01:23.456>`).
fn extract_speaker(text: &str) -> (Option<String>, String) {
    let Some(open_pos) = text.find("<v ") else {
        return (None, strip_tags(text).trim().to_string());
    };
    let after_open = &text[open_pos + 3..];
    let Some(close_rel) = after_open.find('>') else {
        return (None, strip_tags(text).trim().to_string());
    };
    let name_raw = &after_open[..close_rel];
    let name = name_raw.trim().to_string();
    let mut rest = after_open[close_rel + 1..].to_string();
    if let Some(end_tag) = rest.find("</v>") {
        rest.replace_range(end_tag..end_tag + 4, "");
    }
    let cleaned = strip_tags(&rest).trim().to_string();
    let speaker = if name.is_empty() { None } else { Some(name) };
    (speaker, cleaned)
}

/// Strip leftover VTT tags without pulling in a regex dep.
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "WEBVTT\n\n\
        00:00:00.000 --> 00:00:03.500\n\
        <v Host>Welcome to the show.\n\n\
        00:00:03.500 --> 00:00:07.250\n\
        <v Guest>Glad to be here.\n\n\
        00:00:07.250 --> 00:00:10.000\n\
        Plain narration with no speaker.\n";

    #[test]
    fn parses_three_entry_document() {
        let t = parse_vtt(SAMPLE, "ep-1", "https://example.com/t.vtt").unwrap();
        assert_eq!(t.entries.len(), 3);
        assert_eq!(t.entries[0].speaker.as_deref(), Some("Host"));
        assert_eq!(t.entries[0].text, "Welcome to the show.");
        assert_eq!(t.entries[1].speaker.as_deref(), Some("Guest"));
        assert!((t.entries[1].start_secs - 3.5).abs() < 1e-9);
        assert_eq!(t.entries[2].speaker, None);
        assert_eq!(t.kind, TranscriptKind::Vtt);
    }

    #[test]
    fn rejects_input_without_header() {
        let err = parse_vtt("00:00:00.000 --> 00:00:01.000\nHi", "ep-1", "u").unwrap_err();
        assert_eq!(err, ParseError::MissingHeader);
    }

    #[test]
    fn skips_note_blocks() {
        let input = "WEBVTT\n\nNOTE this is a note\n\n00:00:00.000 --> 00:00:01.000\nHi";
        let t = parse_vtt(input, "ep-1", "u").unwrap();
        assert_eq!(t.entries.len(), 1);
        assert_eq!(t.entries[0].text, "Hi");
    }

    #[test]
    fn accepts_cue_settings_on_right_side() {
        let input = "WEBVTT\n\n00:00:00.000 --> 00:00:05.000 align:start\nHello";
        let t = parse_vtt(input, "ep-1", "u").unwrap();
        assert_eq!(t.entries.len(), 1);
        assert!((t.entries[0].end_secs - 5.0).abs() < 1e-9);
    }

    #[test]
    fn strips_inline_tags() {
        let input = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\n<v Host>Hello <c.class>world</c>.";
        let t = parse_vtt(input, "ep-1", "u").unwrap();
        assert_eq!(t.entries[0].text, "Hello world.");
    }
}
