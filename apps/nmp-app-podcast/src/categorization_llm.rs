//! LLM-based episode categorization using rig-core + Ollama (M5.6).
//!
//! [`categorize_episode`] assigns 1–3 category tags to an episode by
//! prompting a local Ollama instance (default: `http://localhost:11434`)
//! and parsing the JSON-array reply against a fixed 15-item taxonomy.
//!
//! The function is synchronous at the call site — the caller supplies the
//! shared Tokio runtime (wrapped in `Arc`) and we use `block_on` so a
//! `spawn_blocking` worker can call it without being async itself. This
//! mirrors `inbox_llm`, `picks_llm`, and `briefing_llm`.
//!
//! ## Taxonomy enforcement
//!
//! The model is *told* to pick from the fixed list, but we never trust the
//! prompt: [`filter_taxonomy`] drops any off-list label, dedups, and caps
//! the result at [`MAX_CATEGORIES_PER_EPISODE`]. If nothing survives the
//! filter the function returns `Err` so the caller falls back to keyword
//! matching.
//!
//! ## Failure handling
//!
//! If Ollama is offline, the response is unparseable, or the filtered
//! result is empty, the function returns `Err(String)`. The caller
//! ([`crate::categorization`]) is expected to keep the keyword-matched tags
//! it already wrote and log the failure without surfacing it to the user.
//!
//! ## Cache-wipe note
//!
//! Categorization uses a single shared `categories` cache (unlike inbox's
//! two-tier projection). Each feed refresh's synchronous keyword pass
//! replaces the cache wholesale, transiently dropping the prior pass's
//! LLM-derived tags until the background pass below re-stamps them. With
//! the re-entrancy guard on the spawn this self-heals on the next refresh;
//! a two-tier projection would be over-engineering for the current scope.

use std::sync::Arc;

use tokio::runtime::Runtime;

use rig_core::client::{CompletionClient as _, Nothing};
use rig_core::completion::Prompt as _;
use rig_core::providers::ollama;

use crate::ffi::actions::categorization_module::MAX_CATEGORIES_PER_EPISODE;

const OLLAMA_BASE_URL: &str = "http://localhost:11434";
const CATEGORIZE_MODEL: &str = "deepseek-v4-flash:cloud";

/// The fixed category taxonomy. The LLM is constrained to this list in the
/// system prompt, and its output is filtered against it (exact, title-case).
pub const TAXONOMY: &[&str] = &[
    "Technology",
    "Business",
    "Science",
    "Health",
    "Politics",
    "Culture",
    "Sports",
    "Education",
    "Entertainment",
    "History",
    "Philosophy",
    "True Crime",
    "Comedy",
    "Finance",
    "Spirituality",
];

const CATEGORIZE_PREAMBLE: &str = "You are a podcast episode categorizer. Given an episode title and description, output 1-3 category tags from this list: [Technology, Business, Science, Health, Politics, Culture, Sports, Education, Entertainment, History, Philosophy, True Crime, Comedy, Finance, Spirituality]. Output ONLY a JSON array of strings: [\"Category1\", \"Category2\"]. No other text.";

/// Score an episode's categories using the LLM.
///
/// Returns 1–3 taxonomy-valid tags, or `Err` if Ollama is unreachable, the
/// response is unparseable, or no in-taxonomy tags survive filtering.
pub fn categorize_episode(
    episode_title: &str,
    description: &str,
    runtime: &Arc<Runtime>,
) -> Result<Vec<String>, String> {
    runtime.block_on(async {
        let client = ollama::Client::builder()
            .api_key(Nothing)
            .base_url(OLLAMA_BASE_URL)
            .build()
            .map_err(|e: rig_core::http_client::Error| e.to_string())?;

        let agent = client
            .agent(CATEGORIZE_MODEL)
            .preamble(CATEGORIZE_PREAMBLE)
            .build();

        let truncated: String = description.chars().take(500).collect();
        let prompt = format!("Title: {episode_title}\nDescription: {truncated}");

        let response: String = agent.prompt(&prompt).await.map_err(|e| e.to_string())?;

        parse_category_array(&response)
    })
}

/// Parse an LLM response into a filtered, taxonomy-valid category list.
///
/// Extracts the first balanced `[…]` JSON array, parses it as
/// `Vec<String>`, then runs [`filter_taxonomy`]. Returns `Err` if no array
/// is present, the array doesn't parse, or nothing survives filtering.
///
/// Split out from the network call so the parsing logic is unit-testable
/// without a live Ollama.
pub fn parse_category_array(response: &str) -> Result<Vec<String>, String> {
    let json_str = extract_json_array(response)?;
    let raw: Vec<String> = serde_json::from_str(&json_str).map_err(|e| e.to_string())?;
    let filtered = filter_taxonomy(raw);
    if filtered.is_empty() {
        return Err("no in-taxonomy categories in LLM response".into());
    }
    Ok(filtered)
}

/// Filter a list of candidate labels to the fixed taxonomy.
///
/// Keeps only exact (title-case) taxonomy matches, dedups while preserving
/// first-seen order, and caps the result at [`MAX_CATEGORIES_PER_EPISODE`].
fn filter_taxonomy(candidates: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(MAX_CATEGORIES_PER_EPISODE);
    for label in candidates {
        let trimmed = label.trim();
        if TAXONOMY.contains(&trimmed) && !out.iter().any(|x| x == trimmed) {
            out.push(trimmed.to_owned());
            if out.len() == MAX_CATEGORIES_PER_EPISODE {
                break;
            }
        }
    }
    out
}

/// Extract the first balanced `[…]` JSON array from an arbitrary string.
///
/// The LLM may wrap its JSON in markdown fences or preamble text; this
/// finds the first `[` and the last `]` and returns just that slice.
fn extract_json_array(s: &str) -> Result<String, String> {
    let start = s.find('[').ok_or("no JSON array found in LLM response")?;
    let end = s.rfind(']').ok_or("no closing bracket in LLM response")?;
    if end < start {
        return Err("malformed JSON: closing bracket before opening bracket".into());
    }
    Ok(s[start..=end].to_owned())
}

#[cfg(test)]
#[path = "categorization_llm_tests.rs"]
mod tests;
