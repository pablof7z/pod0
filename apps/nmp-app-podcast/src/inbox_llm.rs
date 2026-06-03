//! LLM-based inbox triage using rig-core + Ollama.
//!
//! [`triage_episode`] classifies an episode for inbox priority. It calls
//! a local Ollama instance (default: `http://localhost:11434`) and parses
//! the structured JSON reply into [`TriageResult`].
//!
//! The function is synchronous at the call site — the caller supplies the
//! shared Tokio runtime from `PodcastHostOpHandler.runtime` and we use
//! `block_on` so the actor thread can call this without being async itself.
//!
//! ## Failure handling
//!
//! If Ollama is offline or returns an unparseable response the function
//! returns `Err(String)`. The caller records a [`TriageStatus::Pending`]
//! cache entry so `build_inbox` falls back to the recency-bucket heuristic
//! and the proactive trigger retries later under a cooldown — without
//! surfacing the failure to the user.
//!
//! ## Blocking concern
//!
//! `triage_episode` itself uses `runtime.block_on`, but it is only ever
//! invoked from inside `tokio::task::spawn_blocking` on a background task
//! (see `inbox_handler::triage_episodes_in_background`). The actor thread is
//! never blocked: both the explicit `InboxAction::Triage` and the proactive
//! snapshot-path trigger spawn that background task and return immediately.

use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;

use rig_core::client::{CompletionClient as _, Nothing};
use rig_core::completion::Prompt as _;
use rig_core::providers::ollama;

/// Lifecycle status of a cached triage entry.
///
/// The cache stores an entry for an episode whether the LLM call **succeeded**
/// or **failed**, so the proactive trigger can tell "never attempted" apart
/// from "attempted recently, leave it alone." See [`TriageResult`] and
/// `inbox_handler::episodes_needing_triage`.
///
/// `Serialize`/`Deserialize` so the whole [`TriageResult`] (which embeds this)
/// can be persisted to `<data_dir>/inbox-triage-cache.json` and reloaded on a
/// cold launch (`store::inbox_triage_cache`), sparing a full re-triage pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriageStatus {
    /// The LLM produced a usable score; `priority_score` / `priority_reason` /
    /// `categories` are authoritative and `build_inbox` uses them verbatim.
    Ready,
    /// The LLM call failed (Ollama offline, unparseable reply, …). The score
    /// fields are placeholders and `build_inbox` ignores them in favor of the
    /// recency heuristic. The entry exists only to record `attempted_at` so the
    /// proactive trigger applies a retry cooldown instead of re-spawning every
    /// snapshot tick.
    Pending,
}

/// Result of LLM-based episode triage.
///
/// `Serialize`/`Deserialize` so the in-memory triage cache survives an app
/// restart — persisted by `store::inbox_triage_cache` after each triage batch
/// and reloaded in `nmp_app_podcast_set_data_dir`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageResult {
    /// Normalized priority score in the range `0.0..=1.0`.
    pub priority_score: f32,
    /// One-sentence human-readable reason for the score.
    pub priority_reason: String,
    /// Zero or more topic / guest category labels.
    pub categories: Vec<String>,
    /// Whether this entry carries a real LLM score (`Ready`) or is a
    /// failure placeholder awaiting retry (`Pending`).
    pub status: TriageStatus,
    /// Unix seconds when the triage attempt that produced this entry ran.
    /// Drives both 24h staleness (for `Ready`) and the retry cooldown
    /// (for `Pending`) in `inbox_handler::episodes_needing_triage`.
    pub attempted_at: i64,
}

impl TriageResult {
    /// Construct a successful (`Ready`) triage entry stamped at `attempted_at`.
    pub fn ready(
        priority_score: f32,
        priority_reason: String,
        categories: Vec<String>,
        attempted_at: i64,
    ) -> Self {
        Self {
            priority_score,
            priority_reason,
            categories,
            status: TriageStatus::Ready,
            attempted_at,
        }
    }

    /// Construct a failure placeholder (`Pending`) stamped at `attempted_at`.
    /// The score fields are inert; `build_inbox` falls back to the heuristic
    /// for `Pending` entries.
    pub fn pending(attempted_at: i64) -> Self {
        Self {
            priority_score: 0.0,
            priority_reason: String::new(),
            categories: Vec::new(),
            status: TriageStatus::Pending,
            attempted_at,
        }
    }
}

const TRIAGE_MODEL: &str = "deepseek-v4-flash:cloud";

const TRIAGE_PREAMBLE: &str = r#"You are a podcast inbox triage assistant. Given episode metadata, output ONLY valid JSON with these fields: {"priority_score": <0.0-1.0>, "priority_reason": "<one sentence why>", "categories": ["<tag1>", "<tag2>"]}. No other text."#;

/// Classify an episode for inbox priority using a local Ollama LLM.
///
/// Returns `Err` if the Ollama endpoint is unreachable or the model
/// response cannot be parsed as valid triage JSON.
pub fn triage_episode(
    episode_title: &str,
    podcast_title: &str,
    description: &str,
    runtime: &Runtime,
    base_url: &str,
) -> Result<TriageResult, String> {
    runtime.block_on(async {
        let client = ollama::Client::builder()
            .api_key(Nothing)
            .base_url(base_url)
            .build()
            .map_err(|e: rig_core::http_client::Error| e.to_string())?;

        let agent = client
            .agent(TRIAGE_MODEL)
            .preamble(TRIAGE_PREAMBLE)
            .build();

        let truncated: String = description.chars().take(500).collect();
        let prompt = format!(
            "Podcast: {podcast_title}\nEpisode: {episode_title}\nDescription: {truncated}"
        );

        let response: String = agent.prompt(&prompt).await.map_err(|e| e.to_string())?;

        let json_str = extract_json_object(&response)?;
        let v: serde_json::Value =
            serde_json::from_str(&json_str).map_err(|e| e.to_string())?;

        let priority_score = v["priority_score"].as_f64().unwrap_or(0.5) as f32;
        let priority_reason = v["priority_reason"]
            .as_str()
            .unwrap_or("LLM-scored episode")
            .to_owned();
        let categories = v["categories"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();

        Ok(TriageResult::ready(
            priority_score.clamp(0.0, 1.0),
            priority_reason,
            categories,
            chrono::Utc::now().timestamp(),
        ))
    })
}

/// Extract the first `{…}` JSON object from an arbitrary string.
///
/// The LLM may wrap its JSON in markdown fences or preamble text; this
/// finds the outermost balanced `{…}` delimiters and returns just that
/// slice.
fn extract_json_object(s: &str) -> Result<String, String> {
    let start = s.find('{').ok_or("no JSON object found in LLM response")?;
    let end = s.rfind('}').ok_or("no closing brace in LLM response")?;
    if end < start {
        return Err("malformed JSON: closing brace before opening brace".into());
    }
    Ok(s[start..=end].to_owned())
}

#[cfg(test)]
mod tests {
    use super::extract_json_object;

    #[test]
    fn extract_bare_object() {
        let s = r#"{"priority_score":0.9,"priority_reason":"New episode","categories":["tech"]}"#;
        let result = extract_json_object(s).unwrap();
        assert!(result.starts_with('{'));
        assert!(result.ends_with('}'));
    }

    #[test]
    fn extract_object_with_preamble() {
        let s = r#"Sure! Here is the JSON: {"priority_score":0.7,"priority_reason":"Interesting","categories":[]} Great!"#;
        let result = extract_json_object(s).unwrap();
        assert!(result.contains("priority_score"));
    }

    #[test]
    fn extract_fails_on_empty() {
        assert!(extract_json_object("no braces here").is_err());
    }
}
