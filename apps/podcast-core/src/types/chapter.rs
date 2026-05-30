use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

/// Provenance of a [`Chapter`] — where the chapter's title + start offset
/// came from. Lets the UI/projection signal confidence: a transcript-grounded
/// LLM chapter is trustworthy, an equal-length stub is a fallback placeholder.
///
/// `is_ai_generated` can't carry this distinction because both `Llm` and
/// `Stub` chapters are AI-generated; `ChapterSource` is the finer signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ChapterSource {
    /// Publisher-supplied chapters (RSS / Podcasting 2.0). The default so any
    /// chapter decoded from an older snapshot (pre-`source`) reads as publisher.
    #[default]
    Publisher,
    /// Synthesized by the transcript-grounded LLM round-trip.
    Llm,
    /// Equal-length placeholder emitted when the LLM is definitively
    /// unavailable (Ollama unreachable / timed out).
    Stub,
}

impl ChapterSource {
    /// True for the default publisher provenance. Used by projections to skip
    /// the field on the wire when it carries no information.
    pub fn is_publisher(&self) -> bool {
        matches!(self, ChapterSource::Publisher)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    pub id: Uuid,
    pub start_secs: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_secs: Option<f64>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_url: Option<Url>,
    pub include_in_toc: bool,
    pub is_ai_generated: bool,
    /// Provenance of this chapter. Defaults to [`ChapterSource::Publisher`] so
    /// chapters decoded from a pre-`source` snapshot keep wire-compat.
    #[serde(default)]
    pub source: ChapterSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_episode_id: Option<String>,
}

impl Chapter {
    pub fn new(title: impl Into<String>, start_secs: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            start_secs,
            end_secs: None,
            title: title.into(),
            image_url: None,
            link_url: None,
            include_in_toc: true,
            is_ai_generated: false,
            source: ChapterSource::Publisher,
            summary: None,
            source_episode_id: None,
        }
    }
}

#[cfg(test)]
#[path = "chapter_tests.rs"]
mod tests;
