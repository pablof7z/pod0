//! LLM integration for agent chat — synchronous wrapper over rig-core + Ollama.
//!
//! The exported entry point [`chat_with_tools`] drives a blocking, tool-calling
//! Ollama loop from the actor thread (which is a plain `std::thread`, not a
//! Tokio worker). The Tokio runtime is passed in from
//! [`super::host_op_handler::PodcastHostOpHandler`] so the caller can reuse the
//! shared multi-thread scheduler rather than spinning up a new one per call.
//!
//! Model selection follows AGENTS.md:
//! - [`THINKING_MODEL`] for agent-chat turns (reasoning mode).
//! - [`FAST_MODEL`] is the fallback when the primary model is unavailable.

use std::sync::{Arc, Mutex};

use rig_core::client::{CompletionClient, Nothing};
use rig_core::completion::{Chat, Message};
use rig_core::providers::ollama;

use crate::agent_tools::{self, ToolRegistry, TOOL_INSTRUCTIONS};
use crate::store::PodcastStore;

/// Maximum number of tool-call round-trips before we force a final answer.
/// Local models occasionally loop; this bounds latency and token spend.
const MAX_TOOL_TURNS: usize = 3;

/// Fast, low-latency model for iterative requests.
pub const FAST_MODEL: &str = "deepseek-v4-flash:cloud";

/// Thinking/agent model for deep-reasoning chat turns.
pub const THINKING_MODEL: &str = "deepseek-v4-pro:cloud";

/// Ollama base URL used across all LLM requests in the app.
pub const OLLAMA_BASE_URL: &str = "http://localhost:11434";

/// Convert stored `(role, content)` pairs into rig-core chat history.
/// The `Chat` trait prepends the new user turn itself — we only pass prior turns.
fn make_history(pairs: &[(String, String)]) -> Vec<Message> {
    pairs
        .iter()
        .map(|(role, content)| {
            if role == "user" {
                Message::user(content.as_str())
            } else {
                Message::assistant(content.as_str())
            }
        })
        .collect()
}

/// Drive one model turn: thinking model first, fast model as fallback.
/// Shared by [`chat_sync`] and the tool loop in [`chat_with_tools`].
async fn single_turn(
    system_prompt: &str,
    history: &[(String, String)],
    user_message: &str,
) -> Result<String, String> {
    let client = ollama::Client::builder()
        .base_url(OLLAMA_BASE_URL)
        .api_key(Nothing)
        .build()
        .map_err(|e| e.to_string())?;

    // Try the thinking model first (reasoning mode for richer answers).
    let thinking_agent = client.agent(THINKING_MODEL).preamble(system_prompt).build();
    let mut h1 = make_history(history);
    match thinking_agent.chat(user_message, &mut h1).await {
        Ok(reply) => return Ok(reply),
        Err(thinking_err) => {
            eprintln!(
                "agent_llm: {THINKING_MODEL} failed ({thinking_err}), retrying with {FAST_MODEL}"
            );
        }
    }

    // Fall back to the fast model.
    let fast_agent = client.agent(FAST_MODEL).preamble(system_prompt).build();
    let mut h2 = make_history(history);
    fast_agent
        .chat(user_message, &mut h2)
        .await
        .map_err(|e| format!("{FAST_MODEL} also failed: {e}"))
}

/// Drive a chat turn with podcast-domain tools available (M5.4).
///
/// Implements a manual, model-agnostic tool-calling loop. Local models are
/// asked (via [`TOOL_INSTRUCTIONS`] appended to the system prompt) to reply
/// with a single `{"tool":...,"args":{...}}` JSON object when they want to use
/// a tool. We parse that with [`agent_tools::parse_tool_call`], run it against
/// the shared [`PodcastStore`] via [`ToolRegistry`], append the result to the
/// running history as an extra user turn, and re-prompt — up to
/// [`MAX_TOOL_TURNS`] times. The first response that is *not* a tool call is
/// returned as the final answer.
///
/// `history` is the conversation up to but not including `user_message`.
/// Returns `Err` only when the model is unreachable on the very first turn
/// (so the caller can fall back to the scaffold reply). Once a turn has
/// succeeded, later model failures degrade to returning the best text so far.
pub fn chat_with_tools(
    system_prompt: &str,
    history: &[(String, String)],
    user_message: &str,
    store: Arc<Mutex<PodcastStore>>,
    runtime: &tokio::runtime::Runtime,
) -> Result<String, String> {
    let registry = ToolRegistry::new(store);
    let full_prompt = format!("{system_prompt}\n\n{TOOL_INSTRUCTIONS}");

    runtime.block_on(async {
        // Working history that grows with tool calls/results across turns.
        let mut convo: Vec<(String, String)> = history.to_vec();
        // The first turn sends the real user message; subsequent turns re-prompt
        // with the accumulated tool results already folded into `convo`.
        let mut next_user_message = user_message.to_owned();
        let mut used_a_tool = false;

        for _ in 0..MAX_TOOL_TURNS {
            let reply = match single_turn(&full_prompt, &convo, &next_user_message).await {
                Ok(r) => r,
                Err(e) => {
                    // First model call failing means Ollama is down — propagate so
                    // the handler uses its scaffold fallback. If we've already run a
                    // tool, force a clean plain-text summary instead of leaking the
                    // internal "Tool X returned…" scaffolding to the user.
                    if !used_a_tool {
                        return Err(e);
                    }
                    return Ok(force_final_answer(system_prompt, &convo, user_message).await);
                }
            };

            match agent_tools::parse_tool_call(&reply) {
                Some(call) => {
                    used_a_tool = true;
                    let result = registry.execute(&call.name, &call.args);
                    // Record this turn's request + tool result so the next model
                    // turn can see them. The user turn we just sent is recorded as
                    // a user message; the model's tool-call request as assistant.
                    convo.push(("user".to_owned(), std::mem::take(&mut next_user_message)));
                    convo.push(("assistant".to_owned(), reply));
                    next_user_message = format!(
                        "Tool `{}` returned:\n{}\n\nUse this to answer the original question.",
                        call.name, result
                    );
                }
                // Plain-text response: this is the final answer.
                None => return Ok(reply),
            }
        }

        // Tool-call budget exhausted and the model still wants a tool. Make one
        // final tools-suppressed call so the user gets prose, never raw JSON.
        Ok(force_final_answer(system_prompt, &convo, user_message).await)
    })
}

/// Make a final, tools-suppressed model call to summarize the accumulated tool
/// results into a plain-text answer. Uses the *base* system prompt (no tool
/// instructions) so the model answers rather than emitting more tool JSON. On
/// failure, degrades to the scaffold reply rather than leaking internal state.
async fn force_final_answer(
    system_prompt: &str,
    convo: &[(String, String)],
    original_question: &str,
) -> String {
    let closing = format!(
        "Based on the tool results above, answer this question in plain text \
         (do not call any tools): {original_question}"
    );
    single_turn(system_prompt, convo, &closing)
        .await
        .unwrap_or_else(|_| crate::agent_handler::SCAFFOLD_ASSISTANT_REPLY.to_owned())
}
