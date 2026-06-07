//! Scenario: generate a real LLM wiki article via Ollama.
//!
//! Skips automatically when `localhost:11434` is not reachable so the
//! scenario runner stays green in CI environments that don't have Ollama.
//!
//! With Ollama running and `glm-5.1:cloud` available:
//!   - Subscribes to the local mock feed.
//!   - Dispatches `podcast.wiki generate`.
//!   - Waits up to 90 s for an article with a non-placeholder summary.

use nmp_app_podcast::PodcastHandle;
use nmp_ffi::NmpApp;

use crate::harness::{dispatch, probe_tcp, wait_for};
use crate::mock_feed;
use crate::scenarios::llm_setup;
use crate::scenarios::ScenarioResult::{self, Fail, Pass, Skip};

const PLACEHOLDER_MARKER: &str = "LLM synthesis is a follow-up";

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    if !probe_tcp("localhost", 11434) {
        return Skip("ollama offline".into());
    }
    if let Err(err) = llm_setup::configure_glm_ollama(app) {
        return Fail(err);
    }

    // Start local mock feed server; subscribe so the library has a podcast.
    let port = mock_feed::start();
    let feed_url = format!("http://127.0.0.1:{port}/feed.xml");

    let result = dispatch(
        app,
        "podcast",
        serde_json::json!({"op": "subscribe", "feed_url": feed_url}),
    );
    if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("subscribe dispatch rejected: {err}"));
    }

    // Wait for the library to populate.
    let snap = match wait_for(handle, 15_000, |u| !u.library.is_empty()) {
        Ok(s) => s,
        Err(e) => return Fail(format!("library never populated: {e}")),
    };

    let podcast_id = snap.library[0].id.clone();

    // Dispatch wiki generate.
    let result = dispatch(
        app,
        "podcast.wiki",
        serde_json::json!({
            "op": "generate",
            "podcast_id": podcast_id,
            "topic": "main themes"
        }),
    );
    if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("wiki generate dispatch rejected: {err}"));
    }

    // Wait for an article with a real (non-placeholder, non-empty, no error) summary.
    match wait_for(handle, 90_000, |u| {
        u.wiki_articles.iter().any(|a| {
            !a.is_generating
                && !a.summary.is_empty()
                && a.generation_error.is_none()
                && !a.summary.contains(PLACEHOLDER_MARKER)
        })
    }) {
        Ok(u) => {
            let article = u
                .wiki_articles
                .iter()
                .find(|a| {
                    !a.summary.is_empty()
                        && !a.is_generating
                        && a.generation_error.is_none()
                        && !a.summary.contains(PLACEHOLDER_MARKER)
                })
                .unwrap();

            if article.summary.len() < 100 {
                return Fail(format!(
                    "summary too short ({} chars): {:?}",
                    article.summary.len(),
                    article.summary
                ));
            }
            Pass
        }
        Err(e) => Fail(format!("wiki article never appeared: {e}")),
    }
}
