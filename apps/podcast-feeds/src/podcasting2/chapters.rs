use std::fmt;

use podcast_core::{Chapter, ChapterSource};
use serde::Deserialize;
use url::Url;
use uuid::Uuid;

/// Parses a Podcasting 2.0 chapters JSON payload into `Chapter` values.
///
/// Spec: https://github.com/Podcastindex-org/podcast-namespace/blob/main/chapters/jsonChapters.md
///
/// Permissive in the same shape as the Swift `ChaptersClient.decode`:
/// - Integer or floating-point timestamps both decode (`f64`).
/// - Missing optional fields are tolerated.
/// - Entries with no title (or whitespace-only) are skipped — real-world
///   feeds occasionally publish title-less ad markers.
/// - Output is sorted ascending by `start_secs`.
pub fn parse_chapters_json(json: &str) -> Result<Vec<Chapter>, ChaptersError> {
    let envelope: ChaptersEnvelope =
        serde_json::from_str(json).map_err(|e| ChaptersError::Decode(e.to_string()))?;
    let mut out: Vec<Chapter> = envelope
        .chapters
        .into_iter()
        .filter_map(raw_to_chapter)
        .collect();
    out.sort_by(|a, b| a.start_secs.partial_cmp(&b.start_secs).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

fn raw_to_chapter(raw: RawChapter) -> Option<Chapter> {
    let title = raw.title.as_deref().unwrap_or("").trim().to_string();
    if title.is_empty() {
        return None;
    }
    Some(Chapter {
        id: Uuid::new_v4(),
        start_secs: raw.start_time.unwrap_or(0.0),
        end_secs: raw.end_time,
        title,
        image_url: raw.img.as_deref().and_then(|s| Url::parse(s).ok()),
        link_url: raw.url.as_deref().and_then(|s| Url::parse(s).ok()),
        include_in_toc: raw.toc.unwrap_or(true),
        is_ai_generated: false,
        source: ChapterSource::Publisher,
        summary: None,
        source_episode_id: None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChaptersError {
    Decode(String),
}

impl fmt::Display for ChaptersError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChaptersError::Decode(msg) => write!(f, "chapters decode failed: {msg}"),
        }
    }
}

impl std::error::Error for ChaptersError {}

#[derive(Debug, Deserialize)]
struct ChaptersEnvelope {
    #[serde(default)]
    chapters: Vec<RawChapter>,
}

#[derive(Debug, Deserialize)]
struct RawChapter {
    #[serde(rename = "startTime")]
    start_time: Option<f64>,
    #[serde(rename = "endTime")]
    end_time: Option<f64>,
    title: Option<String>,
    img: Option<String>,
    url: Option<String>,
    toc: Option<bool>,
}

#[cfg(test)]
#[path = "chapters_tests.rs"]
mod tests;
