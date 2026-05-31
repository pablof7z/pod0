//! LLM-based picks scoring using rig-core + Ollama (feature #46, M5.6).
//!
//! [`score_episode_for_picks`] scores a single candidate episode for the
//! user-picks rail. It mirrors [`crate::inbox_llm::triage_episode`]: it calls
//! a local Ollama instance (default: `http://localhost:11434`) and parses the
//! structured JSON reply into a `(score, reason)` pair.
//!
//! The function is synchronous at the call site — the caller supplies the
//! shared Tokio runtime (wrapped in `Arc`) and we use `block_on` so a
//! `spawn_blocking` worker can call it without being async itself. This
//! matches the inbox-triage call shape exactly.
//!
//! ## Failure handling
//!
//! If Ollama is offline or returns an unparseable response the function
//! returns `Err(String)`. The caller (`picks_handler`) falls back to the
//! newest-first recency heuristic and logs the failure without surfacing it
//! to the user.
//!
//! ## Why a separate module from `inbox_llm`
//!
//! Inbox triage emits a richer shape (score + reason + category tags) and is
//! consumed by a per-tick projection cache. Picks needs only `(score, reason)`
//! and re-stamps a materialized slot. Keeping the prompt and the parse seam
//! here means the picks recommender prompt can evolve independently of inbox
//! triage without coupling the two features.

use std::sync::Arc;

use tokio::runtime::Runtime;

use rig_core::client::{CompletionClient as _, Nothing};
use rig_core::completion::Prompt as _;
use rig_core::providers::ollama;

/// Ollama endpoint shared with inbox triage.
const OLLAMA_BASE_URL: &str = "http://localhost:11434";
/// Same fast model the inbox triage path uses.
const PICKS_MODEL: &str = "deepseek-v4-flash:cloud";

const PICKS_PREAMBLE: &str = r#"You are a personalized podcast picks recommender. You are given the user's recent listening profile (the shows and topics they actually listen to) followed by a candidate episode. Score the candidate 0.0-1.0 for how well it fits THIS user's tastes — not generic popularity — and give a one-sentence reason that references what they care about. Reward episodes from shows or topics the user already engages with; do not just reward novelty. Output ONLY JSON: {"score": 0.9, "reason": "..."}."#;

/// Score a candidate episode for user picks, personalized against the user's
/// listening profile.
///
/// `listening_profile` is a short, pre-rendered summary of the shows/topics the
/// user actually listens to (see [`crate::picks_handler::build_listening_profile`]).
/// It is injected ahead of the candidate so the LLM ranks for *fit to this user*
/// rather than generic interest — this is the AI-backed "personalized ranking"
/// for feature #46. When the profile is empty (cold start, no history) the prompt
/// degrades to general-interest scoring.
///
/// Returns `(priority_score, reason)` — mirrors
/// [`crate::inbox_llm::triage_episode`]. `priority_score` is clamped to
/// `0.0..=1.0`.
///
/// Returns `Err` if the Ollama endpoint is unreachable or the model response
/// cannot be parsed as valid picks JSON. The caller is expected to fall back
/// to the recency heuristic on `Err`.
pub fn score_episode_for_picks(
    episode_title: &str,
    podcast_title: &str,
    description: &str,
    listening_profile: &str,
    runtime: &Arc<Runtime>,
) -> Result<(f32, String), String> {
    runtime.block_on(async {
        let client = ollama::Client::builder()
            .api_key(Nothing)
            .base_url(OLLAMA_BASE_URL)
            .build()
            .map_err(|e: rig_core::http_client::Error| e.to_string())?;

        let agent = client
            .agent(PICKS_MODEL)
            .preamble(PICKS_PREAMBLE)
            .build();

        let prompt =
            build_picks_prompt(episode_title, podcast_title, description, listening_profile);

        let response: String = agent.prompt(&prompt).await.map_err(|e| e.to_string())?;
        parse_picks_response(&response)
    })
}

/// Compose the user-turn prompt for the picks scorer.
///
/// Lays out the listening profile first (so the model conditions on the user
/// before seeing the candidate), then the candidate episode. Pure + free of
/// the network call so prompt shape is unit-testable. The description is
/// truncated to 500 chars to bound prompt size (matches inbox triage).
pub fn build_picks_prompt(
    episode_title: &str,
    podcast_title: &str,
    description: &str,
    listening_profile: &str,
) -> String {
    let truncated: String = description.chars().take(500).collect();
    let profile = listening_profile.trim();
    let profile_section = if profile.is_empty() {
        "Listener profile: (no listening history yet — score for broad interest)".to_owned()
    } else {
        format!("Listener profile:\n{profile}")
    };
    format!(
        "{profile_section}\n\nCandidate episode:\nPodcast: {podcast_title}\nEpisode: {episode_title}\nDescription: {truncated}"
    )
}

/// Parse the LLM picks reply into `(score, reason)`.
///
/// Tolerant of markdown fences / preamble text via [`extract_json_object`].
/// `score` defaults to `0.5` and `reason` to a generic string if the
/// respective field is missing; `score` is clamped to `0.0..=1.0`. Returns
/// `Err` only when no JSON object can be located at all.
///
/// Pure (no network), so the JSON-shape tests in `picks_llm_tests.rs` exercise
/// it without a live Ollama.
pub fn parse_picks_response(response: &str) -> Result<(f32, String), String> {
    let json_str = extract_json_object(response)?;
    let v: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| e.to_string())?;

    let score = v["score"].as_f64().unwrap_or(0.5) as f32;
    let reason = v["reason"]
        .as_str()
        .unwrap_or("Recommended pick")
        .to_owned();

    Ok((score.clamp(0.0, 1.0), reason))
}

/// Extract the first `{…}` JSON object from an arbitrary string.
///
/// The LLM may wrap its JSON in markdown fences or preamble text; this finds
/// the outermost balanced `{…}` delimiters and returns just that slice.
fn extract_json_object(s: &str) -> Result<String, String> {
    let start = s.find('{').ok_or("no JSON object found in LLM response")?;
    let end = s.rfind('}').ok_or("no closing brace in LLM response")?;
    if end < start {
        return Err("malformed JSON: closing brace before opening brace".into());
    }
    Ok(s[start..=end].to_owned())
}

#[cfg(test)]
#[path = "picks_llm_tests.rs"]
mod tests;
