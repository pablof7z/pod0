//! LLM integration for agent chat and background agent tasks.
//!
//! [`chat_with_tools`] drives the interactive chat tool-calling loop.
//! [`run_background_agent_task`] drives non-interactive background tasks
//! (inbox triage, etc.) using the same agent identity and tool infrastructure
//! but structurally isolated from the conversation transcript.
//!
//! Model selection follows AGENTS.md:
//! - [`THINKING_MODEL`] for agent-chat and triage turns (reasoning mode).
//! - [`FAST_MODEL`] is the fallback when the primary model is unavailable.

use std::sync::{Arc, Mutex};

use crate::agent_tools::{self, ToolRegistry, TOOL_INSTRUCTIONS, TRIAGE_TOOL_INSTRUCTIONS};
use crate::llm::{LlmRequest, backend_for, role_model_or_default};
use crate::store::PodcastStore;

/// Maximum tool-call round-trips for interactive chat turns.
const MAX_TOOL_TURNS: usize = 3;

/// Maximum tool-call round-trips for background agent tasks (triage).
/// Allows: get_memory_facts (1) + search_library (1-2) + set_episode_priorities (1) + headroom.
const MAX_TRIAGE_TOOL_TURNS: usize = 6;

/// Agent identity shared by chat and all background tasks.
pub(crate) const AGENT_SYSTEM_PROMPT: &str =
    "You are a helpful podcast assistant. Answer questions about podcasts, episodes, \
     RSS feeds, and related topics concisely and accurately.";

/// Build the per-turn system prompt, prepending any stored MemoryFacts so the
/// agent carries persistent user context across conversations and background tasks.
///
/// When the store is absent or holds no facts, returns the plain
/// [`AGENT_SYSTEM_PROMPT`] unchanged. Tool instructions are NOT added here —
/// callers append the appropriate instruction block for their context.
pub(crate) fn build_system_prompt_with_memory(store: Option<&Arc<Mutex<PodcastStore>>>) -> String {
    let facts = store
        .and_then(|s| s.lock().ok())
        .map(|s| s.all_memory_facts())
        .unwrap_or_default();

    if facts.is_empty() {
        return AGENT_SYSTEM_PROMPT.to_owned();
    }

    let facts_text: String = facts
        .iter()
        .map(|f| format!("- {}: {}", f.key, f.value))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "{AGENT_SYSTEM_PROMPT}\n\nUser memory facts (things the user has told you):\n{facts_text}"
    )
}

/// Fast, low-latency model for iterative requests.
pub const FAST_MODEL: &str = "deepseek-v4-flash:cloud";

/// Thinking/agent model for deep-reasoning chat turns.
pub const THINKING_MODEL: &str = "deepseek-v4-pro:cloud";

/// Drive one model turn: thinking model first, fast model as fallback.
/// Shared by the tool loop in [`chat_with_tools`].
async fn single_turn(
    system_prompt: &str,
    history: &[(String, String)],
    user_message: &str,
    store: &Arc<Mutex<PodcastStore>>,
) -> Result<String, String> {
    // Try the primary model first (the "Agent (Initial)" role). If the user
    // picked a `local:` model for that role it runs on-device; otherwise the
    // cloud thinking model is used, unchanged.
    let initial_cfg = store
        .lock()
        .ok()
        .map(|s| s.agent_initial_model().to_owned())
        .unwrap_or_default();
    let primary_model = role_model_or_default(&initial_cfg, THINKING_MODEL);
    let backend = backend_for(store, &primary_model);
    let req = LlmRequest {
        system: system_prompt.to_owned(),
        history: history.to_vec(),
        user: user_message.to_owned(),
        model: primary_model.clone(),
    };
    match backend.complete(&req).await {
        Ok(reply) => return Ok(reply),
        Err(thinking_err) => {
            eprintln!(
                "agent_llm: {primary_model} failed ({thinking_err}), retrying with fallback"
            );
        }
    }

    // Fall back to the secondary model (the "Agent (Thinking)" role), honoring
    // a `local:` selection there too; otherwise the cloud fast model.
    let thinking_cfg = store
        .lock()
        .ok()
        .map(|s| s.agent_thinking_model().to_owned())
        .unwrap_or_default();
    let fallback_model = role_model_or_default(&thinking_cfg, FAST_MODEL);
    let backend = backend_for(store, &fallback_model);
    let req = LlmRequest {
        system: system_prompt.to_owned(),
        history: history.to_vec(),
        user: user_message.to_owned(),
        model: fallback_model.clone(),
    };
    backend
        .complete(&req)
        .await
        .map_err(|e| format!("{fallback_model} also failed: {e}"))
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
    let registry = ToolRegistry::new(store.clone());
    let full_prompt = format!("{system_prompt}\n\n{TOOL_INSTRUCTIONS}");

    runtime.block_on(async {
        // Working history that grows with tool calls/results across turns.
        let mut convo: Vec<(String, String)> = history.to_vec();
        // The first turn sends the real user message; subsequent turns re-prompt
        // with the accumulated tool results already folded into `convo`.
        let mut next_user_message = user_message.to_owned();
        let mut used_a_tool = false;

        for _ in 0..MAX_TOOL_TURNS {
            let reply = match single_turn(&full_prompt, &convo, &next_user_message, &store).await {
                Ok(r) => r,
                Err(e) => {
                    // First model call failing means the model is down — propagate so
                    // the handler uses its scaffold fallback. If we've already run a
                    // tool, force a clean plain-text summary instead of leaking the
                    // internal "Tool X returned…" scaffolding to the user.
                    if !used_a_tool {
                        return Err(e);
                    }
                    return Ok(force_final_answer(system_prompt, &convo, user_message, &store).await);
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
        Ok(force_final_answer(system_prompt, &convo, user_message, &store).await)
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
    store: &Arc<Mutex<PodcastStore>>,
) -> String {
    let closing = format!(
        "Based on the tool results above, answer this question in plain text \
         (do not call any tools): {original_question}"
    );
    single_turn(system_prompt, convo, &closing, store)
        .await
        .unwrap_or_else(|_| crate::agent_handler::SCAFFOLD_ASSISTANT_REPLY.to_owned())
}

/// Run a background agent task (e.g. inbox triage) using the full agent
/// identity but structurally isolated from the conversation transcript.
///
/// Uses [`TRIAGE_TOOL_INSTRUCTIONS`] and [`MAX_TRIAGE_TOOL_TURNS`]. The
/// conversation `Arc` is never a parameter — transcript isolation is
/// guaranteed by the type signature, not a runtime guard.
///
/// Returns `Err` only when the model is unreachable on the very first turn;
/// callers should treat that as a total failure and stamp all episodes Pending.
pub fn run_background_agent_task(
    system_prompt: &str,
    user_message: &str,
    store: Arc<Mutex<PodcastStore>>,
    registry: ToolRegistry,
    runtime: &tokio::runtime::Runtime,
) -> Result<String, String> {
    let full_prompt = format!("{system_prompt}\n\n{TRIAGE_TOOL_INSTRUCTIONS}");

    runtime.block_on(async {
        let mut convo: Vec<(String, String)> = Vec::new();
        let mut next_msg = user_message.to_owned();

        for _ in 0..MAX_TRIAGE_TOOL_TURNS {
            let reply = match single_turn(&full_prompt, &convo, &next_msg, &store).await {
                Ok(r) => r,
                Err(e) => {
                    if convo.is_empty() {
                        return Err(e);
                    }
                    // Already made progress — return what we have.
                    return Ok(String::new());
                }
            };

            match agent_tools::parse_tool_call(&reply) {
                Some(call) => {
                    let result = registry.execute(&call.name, &call.args);
                    convo.push(("user".to_owned(), std::mem::take(&mut next_msg)));
                    convo.push(("assistant".to_owned(), reply));
                    next_msg = format!(
                        "Tool `{}` returned:\n{}\n\nContinue with the task.",
                        call.name, result
                    );
                }
                None => return Ok(reply),
            }
        }

        Ok(String::new())
    })
}
