//! Scenario: dispatch `Triage` and verify at least one inbox item receives
//! LLM-assigned categories and a non-heuristic priority reason.
//!
//! Requires a local Ollama instance at `localhost:11434`. If Ollama is
//! unreachable the scenario skips rather than fails so CI isn't blocked by
//! missing infrastructure.

use nmp_app_podcast::PodcastHandle;
use nmp_native_runtime::NmpApp;

use crate::harness::{dispatch, probe_tcp, wait_for};
use crate::mock_feed;
use crate::scenarios::llm_setup;
use crate::scenarios::ScenarioResult::{self, Fail, Pass, Skip};

const PODCAST_NS: &str = "podcast";
const INBOX_NS: &str = "podcast.inbox";
const OLLAMA_HOST: &str = "localhost";
const OLLAMA_PORT: u16 = 11434;
/// Heuristic reason strings that the LLM result must NOT be.
const HEURISTIC_REASONS: &[&str] = &["Just published", "Recent", "This week", "From your library"];

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    // Skip if Ollama isn't reachable — avoids false failures in CI.
    if !probe_tcp(OLLAMA_HOST, OLLAMA_PORT) {
        return Skip("ollama offline".into());
    }
    if let Err(err) = llm_setup::configure_glm_ollama(app) {
        return Fail(err);
    }

    let memory_result = dispatch(
        app,
        "podcast.memory",
        serde_json::json!({
            "op": "remember",
            "key": "episode_preferences",
            "value": "Prioritize technical deep dives, distributed systems, Rust, and practical engineering lessons.",
            "source": "user"
        }),
    );
    if let Some(err) = memory_result.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("memory dispatch rejected: {err}"));
    }

    // Subscribe to a mock feed so we have unlistened episodes to triage.
    let port = mock_feed::start();
    let feed_url = format!("http://127.0.0.1:{port}/feed.xml");

    let result = dispatch(
        app,
        PODCAST_NS,
        serde_json::json!({"op": "subscribe", "feed_url": feed_url}),
    );
    if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("subscribe dispatch rejected: {err}"));
    }

    // Wait for the inbox to populate (subscribe produces inbox items
    // automatically from unlistened episodes).
    match wait_for(handle, 15_000, |u| !u.inbox.is_empty()) {
        Ok(_) => {}
        Err(e) => return Fail(format!("no inbox items after subscribe: {e}")),
    }

    // Dispatch the Triage action. This triggers LLM scoring on the actor
    // thread. glm-5.1:cloud can require backend reprompts before it emits the
    // required tool payload, so the scenario mirrors the shared agent budget.
    dispatch(app, INBOX_NS, serde_json::json!({"op": "triage"}));

    // Wait until at least one inbox item has non-empty ai_categories —
    // that signals the LLM triage has run and been projected.
    match wait_for(handle, 300_000, |u| {
        u.inbox.iter().any(|i| !i.ai_categories.is_empty())
    }) {
        Ok(u) => {
            let triaged = u
                .inbox
                .iter()
                .find(|i| !i.ai_categories.is_empty())
                .unwrap();

            // Verify the reason is LLM-generated, not a fallback bucket.
            let reason = triaged.priority_reason.as_deref().unwrap_or("");
            if HEURISTIC_REASONS.iter().any(|&r| r == reason) {
                return Fail(format!(
                    "priority_reason looks like a heuristic fallback: {reason:?}"
                ));
            }

            Pass
        }
        Err(e) => Fail(format!("no LLM-triaged inbox items after 300 s: {e}")),
    }
}
