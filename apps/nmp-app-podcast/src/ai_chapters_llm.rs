//! LLM-grounded AI chapter synthesis using rig-core + Ollama (M5.5 / M5-chapters).
//!
//! [`synthesize_chapters`] takes a transcript excerpt + episode duration and
//! asks a local Ollama model for a list of `(title, start_secs)` chapter
//! pairs. It mirrors the call shape of [`crate::inbox_llm::triage_episode`]:
//! synchronous at the call site, driven by the shared Tokio runtime via
//! `block_on` so the actor thread can call it without being async.
//!
//! ## Failure handling — typed, so the caller can discriminate
//!
//! The function returns a typed [`SynthError`] rather than a flat string so
//! the caller ([`crate::ai_chapters`]) can tell *why* synthesis failed:
//!
//! * [`SynthError::Unavailable`] — Ollama was unreachable or timed out. The
//!   model is definitively absent, so the equal-length stub is the only
//!   sensible fallback (the feature must still work offline).
//! * [`SynthError::Parse`] — Ollama *answered* but the response couldn't be
//!   parsed into a valid chapter array. The model is present, so a stub would
//!   be lying about confidence; the caller retries with a simpler prompt and,
//!   failing that, surfaces an error rather than fabricating equal slices.
//!
//! The unreachable-vs-answered split does NOT string-match error text (which
//! is brittle across rig/reqwest versions). Instead it's structural: a
//! timeout from [`tokio::time::timeout`] or any error out of `prompt().await`
//! is `Unavailable`; an error out of our own [`parse_chapters`] step (which
//! only runs after a successful round-trip) is `Parse`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::runtime::Runtime;

use crate::llm::{LlmRequest, backend_for, role_model_or_default};
use crate::store::PodcastStore;

const CHAPTERS_MODEL: &str = "deepseek-v4-flash:cloud";

/// Wall-clock budget for a single chapter round-trip. A hung Ollama must not
/// pin a `spawn_blocking` worker forever; on timeout we treat the model as
/// definitively unavailable and let the caller fall back to the stub.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);

/// Why chapter synthesis failed. Drives the caller's fallback decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SynthError {
    /// Ollama was unreachable or the request timed out — no model available.
    Unavailable(String),
    /// Ollama responded but the answer wasn't a usable chapter array.
    Parse(String),
}

impl SynthError {
    /// True when the model is definitively absent (unreachable / timed out),
    /// i.e. the stub fallback is justified. A [`SynthError::Parse`] is *not*
    /// definitive — the model is present and a retry may succeed.
    pub fn is_unavailable(&self) -> bool {
        matches!(self, SynthError::Unavailable(_))
    }

    pub fn message(&self) -> &str {
        match self {
            SynthError::Unavailable(m) | SynthError::Parse(m) => m,
        }
    }
}

impl std::fmt::Display for SynthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SynthError::Unavailable(m) => write!(f, "ollama unavailable: {m}"),
            SynthError::Parse(m) => write!(f, "unparseable chapter response: {m}"),
        }
    }
}

/// A single chapter synthesized by the LLM.
#[derive(Debug, Clone, PartialEq)]
pub struct SynthesizedChapter {
    /// Human-readable chapter title (3–8 words per the system prompt).
    pub title: String,
    /// Start offset in seconds; `0.0` for the first chapter, monotonic after.
    pub start_secs: f64,
}

/// Which prompt variant to send. The first attempt asks the model to ground
/// chapter boundaries in the transcript's actual topic transitions; the retry
/// drops the elaborate framing for a terse JSON-only instruction that small
/// models follow more reliably.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptStyle {
    /// Rich, transcript-grounding instruction (first attempt).
    Grounded,
    /// Minimal "JSON only" instruction (parse-error retry).
    Simple,
}

/// One LLM round-trip: given a transcript excerpt + episode duration, return a
/// list of `(title, start_secs)` chapter pairs.
///
/// Returns [`SynthError::Unavailable`] when the LLM endpoint is unreachable
/// or the request times out, and [`SynthError::Parse`] when the model answered
/// but the response can't be parsed into a non-empty valid chapter array.
pub fn synthesize_chapters(
    episode_title: &str,
    transcript_excerpt: &str,
    duration_secs: f64,
    chapter_count: usize,
    runtime: &std::sync::Arc<Runtime>,
    store: &Arc<Mutex<PodcastStore>>,
) -> Result<Vec<SynthesizedChapter>, SynthError> {
    synthesize_chapters_styled(
        episode_title,
        transcript_excerpt,
        duration_secs,
        chapter_count,
        PromptStyle::Grounded,
        runtime,
        store,
    )
}

/// Round-trip with an explicit [`PromptStyle`]. Split out so the caller's
/// retry path can re-issue the request with [`PromptStyle::Simple`] after a
/// parse failure.
pub(crate) fn synthesize_chapters_styled(
    episode_title: &str,
    transcript_excerpt: &str,
    duration_secs: f64,
    chapter_count: usize,
    style: PromptStyle,
    runtime: &std::sync::Arc<Runtime>,
    store: &Arc<Mutex<PodcastStore>>,
) -> Result<Vec<SynthesizedChapter>, SynthError> {
    let preamble = system_prompt(chapter_count, style);
    let prompt = format!(
        "Episode title: {episode_title}\n\
         Episode duration: {duration_secs} seconds\n\
         Chapters to produce: {chapter_count}\n\n\
         Transcript excerpt (identify where the conversation shifts topic):\n{transcript_excerpt}"
    );

    runtime.block_on(async {
        // Honor a `local:` selection for the Chapter Compilation role;
        // otherwise the cloud chapters model, unchanged.
        let chapters_cfg = store
            .lock()
            .ok()
            .map(|s| s.chapter_compilation_model().to_owned())
            .unwrap_or_default();
        let chapters_model = role_model_or_default(&chapters_cfg, CHAPTERS_MODEL);
        let backend = backend_for(store, &chapters_model);
        let req = LlmRequest {
            system: preamble,
            history: Vec::new(),
            user: prompt,
            model: chapters_model.clone(),
        };

        // Wrap the round-trip in a timeout: a hung backend would otherwise pin
        // the spawn_blocking worker indefinitely and never reach the stub
        // fallback. A timeout is treated as definitively-unavailable.
        let response: String = match tokio::time::timeout(REQUEST_TIMEOUT, backend.complete(&req)).await {
            Ok(Ok(resp)) => resp,
            // The transport failed / model errored → unreachable.
            Ok(Err(e)) => return Err(SynthError::Unavailable(e.to_string())),
            // Deadline elapsed → treat as unavailable.
            Err(_) => {
                return Err(SynthError::Unavailable(format!(
                    "request exceeded {}s budget",
                    REQUEST_TIMEOUT.as_secs()
                )))
            }
        };

        // The model is present; any failure from here is a parse problem.
        parse_chapters(&response).map_err(SynthError::Parse)
    })
}

/// Build the system prompt for the given [`PromptStyle`], substituting the
/// requested chapter count.
fn system_prompt(chapter_count: usize, style: PromptStyle) -> String {
    match style {
        PromptStyle::Grounded => format!(
            "You are a podcast chapter generator. You are given an excerpt of the episode \
             transcript and the episode duration. Read the transcript and identify the points \
             where the conversation transitions to a new topic; place chapter boundaries at those \
             real transitions rather than at evenly-spaced intervals.\n\
             Output ONLY a valid JSON array of chapters:\n\
             [{{\"title\":\"<chapter title>\",\"start_secs\":<float>}},...]\n\
             Rules: produce {chapter_count} chapters; start_secs must be 0.0 for the first chapter \
             and increase monotonically; the last chapter's start_secs must be < the episode \
             duration; titles must be 3-8 words describing that segment's actual content (never \
             generic labels like \"Chapter 1\"). Output no text other than the JSON array."
        ),
        PromptStyle::Simple => format!(
            "Output ONLY a JSON array of exactly {chapter_count} podcast chapters, no prose:\n\
             [{{\"title\":\"<3-8 word title>\",\"start_secs\":<float>}},...]\n\
             First start_secs is 0.0; values increase; titles describe the content."
        ),
    }
}

/// Parse a model response into a list of [`SynthesizedChapter`].
///
/// Tolerates preamble / trailing prose by extracting the outermost balanced
/// `[ … ]` JSON array slice first. Returns `Err` when no array is present, the
/// slice isn't valid JSON, or the array is empty.
pub(crate) fn parse_chapters(response: &str) -> Result<Vec<SynthesizedChapter>, String> {
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
