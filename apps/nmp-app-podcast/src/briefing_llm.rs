//! LLM-based briefing generation using rig-core + Ollama.
//!
//! [`generate_briefing_segments`] takes the user's recent unplayed episodes
//! (podcast title, episode title, description) and asks a local Ollama
//! instance to write a 3–5 item briefing summary. The reply is parsed from a
//! JSON **array** of strings into a `Vec<String>`, one entry per "story".
//!
//! The function is synchronous at the call site — the caller supplies the
//! shared Tokio runtime and we `block_on` so the actor thread (or a
//! `spawn_blocking` worker) can drive it without being async itself. This
//! mirrors [`crate::wiki_llm::synthesize_summary`] and
//! [`crate::inbox_llm::triage_episode`].
//!
//! ## Failure handling
//!
//! Returns `Err(String)` when Ollama is unreachable or the model reply can't
//! be parsed as a JSON array. The caller
//! ([`crate::briefings_handler::handle_generate_briefing`]) catches the error
//! and falls back to [`fallback_segments`], a no-LLM one-segment summary
//! built from the episode titles only, so the briefing always completes.

use std::sync::Arc;

use rig_core::client::{CompletionClient as _, Nothing};
use rig_core::completion::{Prompt as _, PromptError};
use rig_core::providers::ollama;

pub const FAST_MODEL: &str = "deepseek-v4-flash:cloud";
pub const OLLAMA_BASE_URL: &str = "http://localhost:11434";

/// System preamble for the briefing model.
const BRIEFING_PREAMBLE: &str = "You are a podcast briefing assistant. Given recent podcast \
     episodes, write a 3-5 item briefing summary. Each item is 2-3 sentences. Output ONLY a JSON \
     array of strings: [\"Segment 1...\", \"Segment 2...\"]";

/// Max characters of each episode description folded into the prompt — keeps
/// the context window bounded the same way `inbox_llm` truncates at 500.
const DESC_TRUNCATE_CHARS: usize = 500;

/// Generate a podcast briefing from episode titles + descriptions.
///
/// `episode_summaries` is `(podcast_title, episode_title, episode_description)`
/// for the most-recent unplayed episodes (the caller picks the top 10).
///
/// Returns a `Vec<String>` of briefing segments (one per "story") on success,
/// or `Err(message)` when Ollama is unreachable or the reply can't be parsed
/// as a JSON array. On an empty input slice the function short-circuits to a
/// single "nothing new" segment rather than prompting the model with no
/// material.
pub fn generate_briefing_segments(
    episode_summaries: &[(String, String, String)],
    runtime: &Arc<tokio::runtime::Runtime>,
) -> Result<Vec<String>, String> {
    if episode_summaries.is_empty() {
        return Ok(vec![
            "You're all caught up — no new unplayed episodes in your subscriptions right now."
                .to_owned(),
        ]);
    }

    let prompt = build_prompt(episode_summaries);

    runtime.block_on(async {
        let client = ollama::Client::builder()
            .api_key(Nothing)
            .base_url(OLLAMA_BASE_URL)
            .build()
            .map_err(|e| e.to_string())?;

        let agent = client.agent(FAST_MODEL).preamble(BRIEFING_PREAMBLE).build();

        let response: String = agent
            .prompt(prompt.as_str())
            .await
            .map_err(|e: PromptError| e.to_string())?;

        parse_briefing_array(&response)
    })
}

/// Build the user prompt: an enumerated list of recent episodes with
/// truncated descriptions.
fn build_prompt(episode_summaries: &[(String, String, String)]) -> String {
    let mut prompt =
        String::from("Here are my most recent unplayed podcast episodes:\n\n");
    for (i, (podcast_title, ep_title, ep_desc)) in episode_summaries.iter().enumerate() {
        let truncated: String = ep_desc.chars().take(DESC_TRUNCATE_CHARS).collect();
        prompt.push_str(&format!(
            "{n}. Podcast: {podcast_title}\n   Episode: {ep_title}\n   Description: {truncated}\n\n",
            n = i + 1,
        ));
    }
    prompt.push_str(
        "Write the briefing now as a JSON array of 3-5 strings, each 2-3 sentences.",
    );
    prompt
}

/// Parse a JSON **array of strings** out of an arbitrary LLM reply.
///
/// The model may wrap its array in markdown fences or preamble text; this
/// finds the outermost balanced `[ … ]` slice and decodes it as
/// `Vec<String>`. Returns `Err` when no array is present, the slice doesn't
/// decode, or the decoded array is empty.
///
/// Distinct from `inbox_llm::extract_json_object`, which scans for a `{ … }`
/// object — the briefing wire shape is a top-level array.
pub(crate) fn parse_briefing_array(s: &str) -> Result<Vec<String>, String> {
    let start = s.find('[').ok_or("no JSON array found in LLM response")?;
    let end = s.rfind(']').ok_or("no closing bracket in LLM response")?;
    if end < start {
        return Err("malformed JSON: closing bracket before opening bracket".to_owned());
    }
    let slice = &s[start..=end];
    let segments: Vec<String> =
        serde_json::from_str(slice).map_err(|e| format!("parse briefing array: {e}"))?;
    let segments: Vec<String> = segments
        .into_iter()
        .map(|seg| seg.trim().to_owned())
        .filter(|seg| !seg.is_empty())
        .collect();
    if segments.is_empty() {
        return Err("LLM returned an empty briefing array".to_owned());
    }
    Ok(segments)
}

/// No-LLM fallback: build a single briefing segment from episode titles only.
///
/// Used when Ollama is offline or the reply can't be parsed, so the briefing
/// still completes with something readable instead of staying stuck in the
/// `generating` state. Lists up to the first few titles to keep the segment
/// to one or two sentences.
pub(crate) fn fallback_segments(episode_summaries: &[(String, String, String)]) -> Vec<String> {
    if episode_summaries.is_empty() {
        return vec![
            "You're all caught up — no new unplayed episodes in your subscriptions right now."
                .to_owned(),
        ];
    }
    let titles: Vec<String> = episode_summaries
        .iter()
        .take(5)
        .map(|(podcast_title, ep_title, _)| format!("{ep_title} ({podcast_title})"))
        .collect();
    let joined = titles.join("; ");
    vec![format!(
        "You have {count} recent unplayed episode{plural} waiting, including: {joined}.",
        count = episode_summaries.len(),
        plural = if episode_summaries.len() == 1 { "" } else { "s" },
    )]
}

#[cfg(test)]
#[path = "briefing_llm_tests.rs"]
mod tests;
