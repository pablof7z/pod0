//! Scenario: dispatch `Triage` and verify at least one inbox item receives
//! LLM-assigned categories and a non-heuristic priority reason.
//!
//! Requires a local Ollama instance at `localhost:11434`. If Ollama is
//! unreachable the scenario skips rather than fails so CI isn't blocked by
//! missing infrastructure.

use std::net::TcpStream;
use std::time::Duration;

use nmp_app_podcast::PodcastHandle;
use nmp_ffi::NmpApp;

use crate::harness::{dispatch, wait_for};
use crate::mock_feed;
use crate::scenarios::ScenarioResult::{self, Fail, Pass, Skip};

const PODCAST_NS: &str = "podcast";
const INBOX_NS: &str = "podcast.inbox";
const OLLAMA_HOST: &str = "localhost";
const OLLAMA_PORT: u16 = 11434;
/// Heuristic reason strings that the LLM result must NOT be.
const HEURISTIC_REASONS: &[&str] = &["Just published", "Recent", "This week", "From your library"];

/// Check whether Ollama is reachable on the local machine.
///
/// Uses `ToSocketAddrs` resolution so "localhost" resolves correctly.
fn probe_tcp(host: &str, port: u16) -> bool {
    use std::net::ToSocketAddrs;
    let addr = format!("{host}:{port}");
    let Ok(mut addrs) = addr.to_socket_addrs() else {
        return false;
    };
    let Some(socket_addr) = addrs.next() else {
        return false;
    };
    TcpStream::connect_timeout(&socket_addr, Duration::from_millis(500)).is_ok()
}

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    // Skip if Ollama isn't reachable — avoids false failures in CI.
    if !probe_tcp(OLLAMA_HOST, OLLAMA_PORT) {
        return Skip("ollama offline".into());
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
    // thread. With a 3-episode feed against deepseek-v4-flash:cloud it
    // typically completes in under 30 s, but we allow 120 s to be safe.
    dispatch(app, INBOX_NS, serde_json::json!({"op": "triage"}));

    // Wait until at least one inbox item has non-empty ai_categories —
    // that signals the LLM triage has run and been projected.
    match wait_for(handle, 120_000, |u| {
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
        Err(e) => Fail(format!("no LLM-triaged inbox items after 120 s: {e}")),
    }
}
