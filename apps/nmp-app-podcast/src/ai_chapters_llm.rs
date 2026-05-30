//! LLM-grounded AI chapter synthesis using rig-core + Ollama (M5.5).
//!
//! [`synthesize_chapters`] takes a transcript excerpt + episode duration and
//! asks a local Ollama model for a list of `(title, start_secs)` chapter
//! pairs. It mirrors the call shape of [`crate::inbox_llm::triage_episode`]:
//! synchronous at the call site, driven by the shared Tokio runtime via
//! `block_on` so the actor thread can call it without being async.
//!
//! ## Failure handling
//!
//! If Ollama is offline or returns an unparseable / empty response the
//! function returns `Err(String)`. The caller ([`crate::ai_chapters`]) is
//! expected to fall back to the equal-length stub chapters so the feature
//! degrades gracefully when no model is available.

use tokio::runtime::Runtime;

use rig_core::client::{CompletionClient as _, Nothing};
use rig_core::completion::Prompt as _;
use rig_core::providers::ollama;

const OLLAMA_BASE_URL: &str = "http://localhost:11434";
const CHAPTERS_MODEL: &str = "deepseek-v4-flash:cloud";

/// A single chapter synthesized by the LLM.
#[derive(Debug, Clone, PartialEq)]
pub struct SynthesizedChapter {
    /// Human-readable chapter title (3–8 words per the system prompt).
    pub title: String,
    /// Start offset in seconds; `0.0` for the first chapter, monotonic after.
    pub start_secs: f64,
}

/// One LLM round-trip: given a transcript excerpt + episode duration, return a
/// list of `(title, start_secs)` chapter pairs.
///
/// Returns `Err` if the Ollama endpoint is unreachable, the model response
/// can't be parsed as a valid JSON chapter array, or the array is empty.
pub fn synthesize_chapters(
    episode_title: &str,
    transcript_excerpt: &str,
    duration_secs: f64,
    chapter_count: usize,
    runtime: &std::sync::Arc<Runtime>,
) -> Result<Vec<SynthesizedChapter>, String> {
    let preamble = system_prompt(chapter_count);
    let prompt = format!(
        "Episode: {episode_title}\nDuration: {duration_secs}s\nChapters needed: {chapter_count}\nTranscript excerpt:\n{transcript_excerpt}"
    );

    runtime.block_on(async {
        let client = ollama::Client::builder()
            .api_key(Nothing)
            .base_url(OLLAMA_BASE_URL)
            .build()
            .map_err(|e: rig_core::http_client::Error| e.to_string())?;

        let agent = client.agent(CHAPTERS_MODEL).preamble(&preamble).build();

        let response: String = agent.prompt(&prompt).await.map_err(|e| e.to_string())?;

        parse_chapters(&response)
    })
}

/// Build the verbatim system prompt, substituting the requested chapter count.
fn system_prompt(chapter_count: usize) -> String {
    format!(
        "You are a podcast chapter generator. Given a transcript excerpt and episode duration, output ONLY valid JSON array of chapters:\n\
[{{\"title\":\"<chapter title>\",\"start_secs\":<float>}},...]\n\
Rules: exactly {chapter_count} chapters, start_secs must be 0.0 for first chapter and increase monotonically, last chapter start_secs must be < duration, titles should be 3-8 words describing the content. No other text."
    )
}

/// Parse a model response into a list of [`SynthesizedChapter`].
///
/// Tolerates preamble / trailing prose by extracting the outermost balanced
/// `[ … ]` JSON array slice first. Returns `Err` when no array is present, the
/// slice isn't valid JSON, or the array is empty.
fn parse_chapters(response: &str) -> Result<Vec<SynthesizedChapter>, String> {
    let json_str = extract_json_array(response)?;
    let v: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| e.to_string())?;
    let arr = v
        .as_array()
        .ok_or("LLM response was not a JSON array")?;
    if arr.is_empty() {
        return Err("LLM returned an empty chapter array".into());
    }

    let mut chapters = Vec::with_capacity(arr.len());
    for item in arr {
        let title = item["title"]
            .as_str()
            .ok_or("chapter missing string `title`")?
            .to_owned();
        let start_secs = item["start_secs"]
            .as_f64()
            .ok_or("chapter missing numeric `start_secs`")?;
        if start_secs < 0.0 {
            return Err(format!("chapter '{title}' has negative start_secs ({start_secs})"));
        }
        chapters.push(SynthesizedChapter { title, start_secs });
    }

    // Enforce monotonic ordering: a hallucinating model may return inverted
    // timestamps which break chapter-seek behavior. Sort rather than reject
    // so a small model reordering is corrected rather than discarded.
    chapters.sort_by(|a, b| a.start_secs.partial_cmp(&b.start_secs).unwrap_or(std::cmp::Ordering::Equal));

    // First chapter must start at 0.0 (per the system prompt contract).
    if let Some(first) = chapters.first_mut() {
        first.start_secs = 0.0;
    }

    Ok(chapters)
}

/// Extract the first `[ … ]` JSON array slice from an arbitrary string.
///
/// The LLM may wrap its JSON in markdown fences or preamble text; this finds
/// the outermost `[` … `]` delimiters and returns just that slice.
fn extract_json_array(s: &str) -> Result<String, String> {
    let start = s.find('[').ok_or("no JSON array found in LLM response")?;
    let end = s.rfind(']').ok_or("no closing bracket in LLM response")?;
    if end < start {
        return Err("malformed JSON: closing bracket before opening bracket".into());
    }
    Ok(s[start..=end].to_owned())
}

#[cfg(test)]
#[path = "ai_chapters_llm_tests.rs"]
mod tests;
