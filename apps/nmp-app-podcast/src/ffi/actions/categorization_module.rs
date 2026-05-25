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
mod tests {
    use super::*;

    #[test]
    fn action_ids_match_documented_strings() {
        assert_eq!(ACTION_CATEGORIZE_RUN, "podcast.categorize.run");
        assert_eq!(
            ACTION_CATEGORIZE_EPISODE,
            "podcast.categorize.categorize_episode"
        );
    }

    #[test]
    fn run_action_round_trips() {
        let action = CategorizationAction::Run;
        let json = serde_json::to_string(&action).expect("encode");
        assert_eq!(json, r#"{"op":"run"}"#);
        let decoded: CategorizationAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn categorize_episode_action_round_trips() {
        let action = CategorizationAction::CategorizeEpisode {
            episode_id: "ep-7".into(),
        };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"categorize_episode""#));
        assert!(json.contains(r#""episode_id":"ep-7""#));
        let decoded: CategorizationAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn execute_emits_dispatch_host_op() {
        let action = CategorizationAction::Run;
        let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
        CategorizationModule::execute(action, "corr-1", &|cmd| {
            commands.lock().unwrap().push(cmd);
        })
        .expect("execute ok");
        let commands = commands.into_inner().unwrap();
        assert_eq!(commands.len(), 1);
        let ActorCommand::DispatchHostOp {
            action_json,
            correlation_id,
        } = &commands[0]
        else {
            panic!("expected DispatchHostOp");
        };
        assert_eq!(correlation_id, "corr-1");
        let v: serde_json::Value = serde_json::from_str(action_json).expect("json");
        assert_eq!(v["op"], "run");
    }

    // ── Heuristic categorizer ─────────────────────────────────────────

    #[test]
    fn categorize_text_returns_empty_for_empty_input() {
        assert!(categorize_text("", "").is_empty());
        assert!(categorize_text("   ", "  ").is_empty());
    }

    #[test]
    fn categorize_text_picks_technology_for_software_keywords() {
        let cats = categorize_text(
            "The future of software engineering",
            "We dig into AI, machine learning, and open source tools developers love.",
        );
        assert!(cats.contains(&"Technology".to_owned()));
    }

    #[test]
    fn categorize_text_picks_science_for_research_keywords() {
        let cats = categorize_text(
            "Quantum biology",
            "A scientist explains how quantum physics meets biology in modern research.",
        );
        assert!(cats.contains(&"Science".to_owned()));
    }

    #[test]
    fn categorize_text_picks_business_for_finance_keywords() {
        let cats = categorize_text(
            "Startup finance 101",
            "Investors, founders, and ceos talk vc economy and ipo strategy.",
        );
        assert!(cats.contains(&"Business".to_owned()));
    }

    #[test]
    fn categorize_text_caps_at_three_categories() {
        // A pathological episode hitting every category. Still capped.
        let cats = categorize_text(
            "Tech politics health sports culture business science education entertainment",
            "Code election medicine football art finance research school comedy.",
        );
        assert!(
            cats.len() <= MAX_CATEGORIES_PER_EPISODE,
            "got {} categories: {:?}",
            cats.len(),
            cats
        );
    }

    #[test]
    fn categorize_text_orders_by_match_count() {
        // Two science hits, one technology hit ⇒ Science first.
        let cats = categorize_text(
            "Quantum physics research",
            "An exploration of biology and ai in the lab.",
        );
        // Both should be present.
        assert!(cats.contains(&"Science".to_owned()));
        assert!(cats.contains(&"Technology".to_owned()));
        // Science (3 hits: quantum, physics, biology, research) should
        // outrank Technology (1 hit: ai).
        let sci_idx = cats.iter().position(|c| c == "Science").unwrap();
        let tech_idx = cats.iter().position(|c| c == "Technology").unwrap();
        assert!(sci_idx < tech_idx, "expected Science before Technology in {:?}", cats);
    }

    #[test]
    fn categorize_text_returns_empty_for_unmatched_input() {
        let cats = categorize_text("Lorem ipsum", "dolor sit amet consectetur");
        assert!(cats.is_empty(), "got {:?}", cats);
    }

    #[test]
    fn contains_word_bounded_respects_word_boundaries() {
        // "ai" must not match inside "main" or "said".
        assert!(!contains_word_bounded("the main idea", "ai"));
        assert!(!contains_word_bounded("she said hi", "ai"));
        assert!(contains_word_bounded("we use ai today", "ai"));
        assert!(contains_word_bounded("ai is everywhere", "ai"));
        assert!(contains_word_bounded("everywhere is ai", "ai"));
    }

    #[test]
    fn contains_word_bounded_handles_multiword_needles() {
        assert!(contains_word_bounded(
            "the open source community thrives",
            "open source"
        ));
        assert!(!contains_word_bounded(
            "wide-open sourcing today",
            "open source"
        ));
    }
}
