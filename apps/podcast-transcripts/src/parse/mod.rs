//! Transcript parsers.
//!
//! Each submodule handles one on-the-wire format. All parsers produce the
//! same [`crate::types::Transcript`] shape so downstream chunking and
//! indexing don't care where the bytes came from.
//!
//! Parsers are **pure** — no I/O. Callers fetch bytes via
//! `nmp.http.capability` (M5) and pass the string/bytes here. Errors are
//! reported through [`ParseError`], a small `enum` rather than a `thiserror`
//! crate dep to stay lightweight per crate doctrine.

use std::fmt;

pub mod json;
pub mod srt;
pub mod vtt;

pub use json::parse_podcasting_json;
pub use srt::parse_srt;
pub use vtt::parse_vtt;

/// Error returned by every parser in this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The WebVTT header (`WEBVTT`) was missing.
    MissingHeader,
    /// A timing line could not be parsed.
    MalformedTiming(String),
    /// The body did not contain any cues / segments to parse.
    Empty,
    /// JSON-shaped input that did not decode.
    InvalidJson(String),
    /// JSON input missing the required `segments` array.
    MissingSegments,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::MissingHeader => write!(f, "missing WEBVTT header"),
            ParseError::MalformedTiming(line) => write!(f, "malformed timing line: {line}"),
            ParseError::Empty => write!(f, "transcript input is empty"),
            ParseError::InvalidJson(msg) => write!(f, "invalid JSON transcript: {msg}"),
            ParseError::MissingSegments => write!(f, "JSON transcript missing `segments` array"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Normalises line endings to `\n` so parsers can split blocks on `\n\n`
/// without worrying about CRLF / lone-CR sources.
pub(crate) fn normalise_newlines(source: &str) -> String {
    source.replace("\r\n", "\n").replace('\r', "\n")
}

/// Parses `HH:MM:SS.mmm`, `MM:SS.mmm`, or the SRT comma variant
/// `HH:MM:SS,mmm` into seconds. Shared by VTT and SRT.
pub(crate) fn parse_timestamp(raw: &str) -> Result<f64, ParseError> {
    let cleaned = raw.trim();
    let pieces: Vec<&str> = cleaned.split(':').collect();
    if pieces.len() != 2 && pieces.len() != 3 {
        return Err(ParseError::MalformedTiming(raw.to_string()));
    }

    let (hours, minutes, seconds_raw) = if pieces.len() == 3 {
        let h: f64 = pieces[0]
            .parse()
            .map_err(|_| ParseError::MalformedTiming(raw.to_string()))?;
        let m: f64 = pieces[1]
            .parse()
            .map_err(|_| ParseError::MalformedTiming(raw.to_string()))?;
        (h, m, pieces[2])
    } else {
        let m: f64 = pieces[0]
            .parse()
            .map_err(|_| ParseError::MalformedTiming(raw.to_string()))?;
        (0.0, m, pieces[1])
    };

    let secs: f64 = seconds_raw
        .replace(',', ".")
        .parse()
        .map_err(|_| ParseError::MalformedTiming(raw.to_string()))?;

    Ok(hours * 3600.0 + minutes * 60.0 + secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_three_part_timestamp() {
        let t = parse_timestamp("01:02:03.500").unwrap();
        assert!((t - 3723.5).abs() < 1e-9);
    }

    #[test]
    fn parses_two_part_timestamp() {
        let t = parse_timestamp("02:30.000").unwrap();
        assert!((t - 150.0).abs() < 1e-9);
    }

    #[test]
    fn parses_srt_comma_decimal() {
        let t = parse_timestamp("00:00:01,250").unwrap();
        assert!((t - 1.25).abs() < 1e-9);
    }

    #[test]
    fn rejects_garbage_timestamp() {
        assert!(matches!(
            parse_timestamp("not-a-time"),
            Err(ParseError::MalformedTiming(_))
        ));
    }

    #[test]
    fn normalises_crlf() {
        let input = "a\r\nb\rc\nd";
        assert_eq!(normalise_newlines(input), "a\nb\nc\nd");
    }
}
