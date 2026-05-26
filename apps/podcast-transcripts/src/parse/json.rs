//! Podcasting 2.0 JSON transcript parser.
//!
//! Ported from the legacy
//! `App/Sources/Transcript/Parsing/PodcastingTranscriptJSONParser.swift`.
//!
//! On-the-wire shape:
//!
//! ```json
//! {
//!   "version": "1.0.0",
//!   "language": "en-US",
//!   "segments": [
//!     {
//!       "speaker": "Tim",
//!       "startTime": 0.0,
//!       "endTime": 3.4,
//!       "body": "Welcome back to the show.",
//!       "words": [
//!         { "word": "Welcome", "startTime": 0.0, "endTime": 0.5 }
//!       ]
//!     }
//!   ]
//! }
//! ```
//!
//! We accept the variants found in the wild:
//!
//! - Numeric fields encoded as numbers OR strings (some publishers stringify).
//! - `body` OR `text` for the segment body.
//! - `startTime`/`endTime` OR `start`/`end`.
//! - `word` OR `text` inside per-word entries.

use serde_json::Value;

use crate::parse::ParseError;
use crate::types::{
    Transcript, TranscriptEntry, TranscriptKind, TranscriptSource, TranscriptState, TranscriptWord,
};

/// Parse a Podcasting 2.0 JSON transcript.
pub fn parse_podcasting_json(
    bytes: &[u8],
    episode_id: impl Into<String>,
    source_url: impl Into<String>,
) -> Result<Transcript, ParseError> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| ParseError::InvalidJson(e.to_string()))?;
    let object = value.as_object().ok_or(ParseError::InvalidJson(
        "top-level value is not an object".into(),
    ))?;

    let raw_segments = object
        .get("segments")
        .and_then(Value::as_array)
        .ok_or(ParseError::MissingSegments)?;

    let mut entries: Vec<TranscriptEntry> = Vec::with_capacity(raw_segments.len());

    for raw in raw_segments {
        let Some(obj) = raw.as_object() else { continue };
        let Some(text) = obj
            .get("body")
            .and_then(Value::as_str)
            .or_else(|| obj.get("text").and_then(Value::as_str))
        else {
            continue;
        };
        let Some(start) = double_value(obj.get("startTime")).or_else(|| double_value(obj.get("start")))
        else {
            continue;
        };
        let Some(end) =
            double_value(obj.get("endTime")).or_else(|| double_value(obj.get("end")))
        else {
            continue;
        };

        let speaker = obj
            .get("speaker")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let words = obj.get("words").and_then(Value::as_array).map(|arr| {
            arr.iter()
                .filter_map(parse_word)
                .collect::<Vec<TranscriptWord>>()
        });

        entries.push(TranscriptEntry {
            start_secs: start,
            end_secs: end,
            speaker,
            text: text.to_string(),
            words,
        });
    }

    entries.sort_by(|a, b| {
        a.start_secs
            .partial_cmp(&b.start_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let language = object
        .get("language")
        .and_then(Value::as_str)
        .unwrap_or("en-US")
        .to_string();

    Ok(Transcript {
        episode_id: episode_id.into(),
        entries,
        source_url: source_url.into(),
        kind: TranscriptKind::Json,
        status: TranscriptState::Ready {
            source: TranscriptSource::Publisher,
        },
        language,
    })
}

fn parse_word(value: &Value) -> Option<TranscriptWord> {
    let obj = value.as_object()?;
    let text = obj
        .get("word")
        .and_then(Value::as_str)
        .or_else(|| obj.get("text").and_then(Value::as_str))?;
    let start = double_value(obj.get("startTime")).or_else(|| double_value(obj.get("start")))?;
    let end = double_value(obj.get("endTime")).or_else(|| double_value(obj.get("end")))?;
    Some(TranscriptWord {
        start_secs: start,
        end_secs: end,
        text: text.to_string(),
    })
}

/// JSON numbers may arrive as integers, floats, or stringified — accept all.
fn double_value(value: Option<&Value>) -> Option<f64> {
    let v = value?;
    if let Some(n) = v.as_f64() {
        return Some(n);
    }
    if let Some(n) = v.as_i64() {
        return Some(n as f64);
    }
    if let Some(n) = v.as_u64() {
        return Some(n as f64);
    }
    if let Some(s) = v.as_str() {
        return s.parse::<f64>().ok();
    }
    None
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;