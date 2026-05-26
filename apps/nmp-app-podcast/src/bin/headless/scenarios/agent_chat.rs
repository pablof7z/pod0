//! Scenario: send a message to the agent and verify a real LLM reply.
//!
//! Skips gracefully when Ollama is not reachable so CI (which has no Ollama
//! instance) continues to pass. The scenario fails if the model returns the
//! scaffold fallback reply — that would indicate the LLM path is broken while
//! Ollama is available.

use nmp_app_podcast::PodcastHandle;
use nmp_ffi::NmpApp;
use serde_json::json;

use super::ScenarioResult::{self, Fail, Pass, Skip};
use crate::harness::{dispatch, probe_tcp, wait_for};

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    // Gate on Ollama availability. Without it the LLM path cannot be exercised.
    if !probe_tcp("localhost", 11434) {
        return Skip("ollama offline".into());
    }

    // Send a short, well-defined question so the model produces a fast reply.
    // The AgentChatAction uses `"op":"send"` as the discriminator (not "type").
    let res = dispatch(
        app,
        "podcast.agent",
        json!({"op": "send", "message": "What is RSS in one sentence?"}),
    );
    // Async actions return {"correlation_id":"..."} on acceptance.
    // An immediate rejection returns {"error":"..."}.
    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("Send rejected: {err}"));
    }

    // Wait up to 60 s for the agent to finish: busy=false, 2 messages, not generating.
    match wait_for(handle, 60_000, |u| {
        u.agent.as_ref().is_some_and(|a| {
            !a.is_busy
                && a.messages.len() >= 2
                && !a.messages[1].is_generating
                && !a.messages[1].content.is_empty()
        })
    }) {
        Ok(u) => {
            let reply = &u.agent.as_ref().unwrap().messages[1].content;
            // Fail if we got the scaffold fallback — that means LLM integration is broken.
            if reply.as_str() == nmp_app_podcast::agent_handler::SCAFFOLD_ASSISTANT_REPLY {
                return Fail("got scaffold reply — LLM not connected".into());
            }
            if reply.len() < 20 {
                return Fail(format!("reply too short ({} chars): {reply:?}", reply.len()));
            }
            Pass
        }
        Err(e) => Fail(e),
    }
}
