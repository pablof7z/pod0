//! Compound categorization ActionModule — routes all `"podcast.categorize.*"`
//! dispatches.
//!
//! The categorizer is intentionally *heuristic* for now: each episode's
//! `title + description` is tokenized and matched against keyword sets per
//! category (Technology, Science, Business, etc.). An episode picks up at
//! most three category labels, in priority order of keyword-match count.
//! Real LLM-driven classification is a follow-up; this module exists so
//! the UI has a stable contract + a stable on-snapshot projection to
//! render against today.
//!
//! Per D7 the iOS side only **dispatches** the request; the kernel decides
//! which categories exist, which keywords map to which category, and how
//! many labels to assign. Swift never names a category string of its own.
//!
//! ## Wire shape
//!
//! ```text
//! podcast.categorize.run                 — RunCategorization
//! podcast.categorize.categorize_episode  — CategorizeEpisode { episode_id }
//! ```
//!
//! `run` re-categorizes the whole library (called automatically at the
//! end of every successful feed refresh — see [`crate::host_op_handler`]).
//! `categorize_episode` rescans a single episode (useful when the iOS
//! shell wants to refresh one row without rebuilding the projection).

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::ActorCommand;

pub use super::categorization_keywords::CATEGORY_KEYWORDS;

/// Wire enum for all `"podcast.categorize"` namespace actions.
///
/// `#[serde(tag = "op", rename_all = "snake_case")]` maps the JSON
/// discriminator to the lowercase snake-case variant name:
/// `run` → `{"op":"run"}`,
/// `categorize_episode` → `{"op":"categorize_episode","episode_id":"…"}`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum CategorizationAction {
    /// Re-run the heuristic categorizer over every episode in the
    /// kernel-side store. Returns `{"ok":true}` once the projection
    /// cache has been replaced.
    Run,
    /// Re-categorize a single episode. Returns
    /// `{"ok":true,"categories":[...]}` with the labels assigned to
    /// the episode. Useful for the iOS shell to refresh one row
    /// without rebuilding the whole projection.
    CategorizeEpisode { episode_id: String },
}

/// `podcast.categorize.run` action id.
pub const ACTION_CATEGORIZE_RUN: &str = "podcast.categorize.run";

/// `podcast.categorize.categorize_episode` action id.
pub const ACTION_CATEGORIZE_EPISODE: &str = "podcast.categorize.categorize_episode";

/// Single action module for the whole `"podcast.categorize"` namespace.
///
/// `execute` serializes the typed [`CategorizationAction`] back to JSON
/// and forwards the whole action through `ActorCommand::DispatchHostOp`,
/// matching the pattern used by [`super::podcast_module::PodcastActionModule`].
pub struct CategorizationModule;

impl ActionModule for CategorizationModule {
    const NAMESPACE: &'static str = "podcast.categorize";

    type Action = CategorizationAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        let action_json = serde_json::to_string(&action).map_err(|e| e.to_string())?;
        send(ActorCommand::DispatchHostOp {
            action_json,
            correlation_id: correlation_id.to_owned(),
        });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Heuristic categorizer
// ---------------------------------------------------------------------------

/// Maximum number of category labels assigned to a single episode.
pub const MAX_CATEGORIES_PER_EPISODE: usize = 3;

/// Heuristically categorize one episode by `title + description`.
///
/// Returns up to [`MAX_CATEGORIES_PER_EPISODE`] labels, ordered by
/// keyword-match count (highest first). Categories with zero matches
/// are dropped. Ties resolved by [`CATEGORY_KEYWORDS`] order (the
/// stable sort below preserves the canonical order on equal counts).
pub fn categorize_text(title: &str, description: &str) -> Vec<String> {
    let haystack = format!("{} {}", title, description).to_ascii_lowercase();
    if haystack.trim().is_empty() {
        return Vec::new();
    }

    let mut hits: Vec<(usize, &str, usize)> = CATEGORY_KEYWORDS
        .iter()
        .enumerate()
        .map(|(idx, (category, keywords))| {
            let count: usize = keywords
                .iter()
                .filter(|kw| contains_word_bounded(&haystack, kw))
                .count();
            (idx, *category, count)
        })
        .filter(|(_, _, count)| *count > 0)
        .collect();

    // Stable sort: ties keep the canonical CATEGORY_KEYWORDS order.
    hits.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));

    hits.into_iter()
        .take(MAX_CATEGORIES_PER_EPISODE)
        .map(|(_, cat, _)| cat.to_owned())
        .collect()
}

/// Return `true` iff `needle` appears in `haystack` with non-alphanumeric
/// neighbours on both ends (so `"ai"` does not match `"main"`).
///
/// Caller must lowercase both inputs; matching is byte-wise (sufficient
/// for the ASCII keyword sets we ship).
fn contains_word_bounded(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    let bytes = haystack.as_bytes();
    let nlen = needle.len();
    let nb = needle.as_bytes();
    let mut i = 0;
    while i + nlen <= bytes.len() {
        if &bytes[i..i + nlen] == nb {
            let prev_ok = i == 0 || !is_word_byte(bytes[i - 1]);
            let next_ok = i + nlen == bytes.len() || !is_word_byte(bytes[i + nlen]);
            if prev_ok && next_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
#[path = "categorization_module_tests.rs"]
mod tests;
