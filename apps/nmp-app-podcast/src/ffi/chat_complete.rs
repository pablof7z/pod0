//! `nmp_app_podcast_chat_complete` — synchronous single-turn LLM completion.
//!
//! Swift drives the full agent turn-loop (tool dispatch, skills, streaming UI)
//! itself; this entry point replaces the provider-aware Swift HTTP clients
//! (`AgentOpenRouterClient`, `AgentOllamaClient`) with a provider-blind Rust
//! call so Swift is completely unaware of OpenRouter vs. Ollama.
//!
//! ## Wire protocol
//!
//! * **`messages_json`**: a JSON array of OpenAI-format message objects,
//!   e.g. `[{"role":"system","content":"…"},{"role":"user","content":"…"}]`.
//!   `tool_calls` and `tool` role messages are passed through unchanged
//!   so the existing Swift multi-turn tool loop works without modification.
//! * **Return value**: a heap-allocated nul-terminated JSON string of the form
//!   `{"text":"<assistant reply>"}` on success,
//!   `{"error":"<reason>"}` on failure.
//!   The caller MUST free the pointer via `nmp_free_string`.
//!   Never returns NULL for a valid `handle` (D6).
//!
//! ## Threading model
//!
//! The call is **synchronous** from Swift's perspective. Swift MUST wrap it in
//! a detached `Task` / `DispatchQueue` to avoid blocking `@MainActor`. Rust
//! internally drives the call via `runtime.block_on`, reusing the same shared
//! Tokio runtime the host-op handler and voice manager hold.
//!
//! ## D6 — degrade silently
//!
//! Null pointers, invalid UTF-8, JSON decode failures, and lock poison all
//! return an `{"error":"…"}` envelope rather than NULL or a panic. Callers
//! treat any `error` key as a failed turn.

use std::ffi::{c_char, CStr, CString};
use std::sync::Arc;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use crate::agent_llm::chat_with_tools;

/// Convert the OpenAI message array JSON to a flat (role, content) history
/// suitable for the tool loop. System message is extracted separately.
///
/// Strategy: walk the array in order; the first `system` entry becomes the
/// system prompt. All `user` and `assistant` text messages become history
/// pairs. `tool` role messages are formatted as
/// `[tool result for <tool_call_id>]: <content>` and treated as user turns
/// so the model keeps context. `tool_calls` on assistant messages are
/// stringified as `[called tool <name>]` so the model knows what happened.
fn decode_messages(value: &serde_json::Value) -> Option<(String, Vec<(String, String)>)> {
    let arr = value.as_array()?;
    let mut system = String::new();
    let mut history: Vec<(String, String)> = Vec::new();

    for msg in arr {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        match role {
            "system" => {
                let content = msg
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_owned();
                if system.is_empty() {
                    system = content;
                }
            }
            "user" => {
                let content = msg
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_owned();
                if !content.is_empty() {
                    history.push(("user".to_owned(), content));
                }
            }
            "assistant" => {
                // Prefer text content; if absent (tool-call-only turn),
                // stringify the tool calls so the model sees context.
                let text = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
                if !text.is_empty() {
                    history.push(("assistant".to_owned(), text.to_owned()));
                } else if let Some(calls) = msg.get("tool_calls").and_then(|tc| tc.as_array()) {
                    let names: Vec<&str> = calls
                        .iter()
                        .filter_map(|c| {
                            c.get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(|n| n.as_str())
                        })
                        .collect();
                    if !names.is_empty() {
                        let summary = format!("[called tool: {}]", names.join(", "));
                        history.push(("assistant".to_owned(), summary));
                    }
                }
            }
            "tool" => {
                // Tool results — treat as user turn so the model sees them.
                let call_id = msg
                    .get("tool_call_id")
                    .and_then(|id| id.as_str())
                    .unwrap_or("unknown");
                let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
                let text = format!("[tool result for {call_id}]: {content}");
                history.push(("user".to_owned(), text));
            }
            _ => {}
        }
    }
    Some((system, history))
}

/// Encode the reply as `{"text":"<content>"}`.
fn ok_envelope(text: &str) -> CString {
    let json = serde_json::json!({"text": text}).to_string();
    CString::new(json).unwrap_or_else(|_| CString::new(r#"{"error":"encoding"}"#).unwrap())
}

/// Encode an error as `{"error":"<reason>"}`.
fn err_envelope(reason: &str) -> CString {
    let safe = reason.replace('"', "'");
    CString::new(format!(r#"{{"error":"{safe}"}}"#))
        .unwrap_or_else(|_| CString::new(r#"{"error":"encoding"}"#).unwrap())
}

/// Execute a single-turn LLM completion and return the assistant's text.
///
/// Takes the full OpenAI message array (system + history + latest user turn)
/// as a JSON string. Returns a heap-allocated `{"text":"…"}` or
/// `{"error":"…"}` JSON string. Caller MUST free via `nmp_free_string`.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_chat_complete(
    handle: *mut PodcastHandle,
    messages_json: *const c_char,
) -> *mut c_char {
    // D6: null handle or null input → error envelope (never NULL return).
    if handle.is_null() || messages_json.is_null() {
        return err_envelope("null argument").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_chat_complete",
        || err_envelope("panic").into_raw(),
        || {
            let json_str = match unsafe { CStr::from_ptr(messages_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return err_envelope("invalid UTF-8").into_raw(),
            };

            let parsed: serde_json::Value = match serde_json::from_str(json_str) {
                Ok(v) => v,
                Err(e) => return err_envelope(&format!("JSON parse: {e}")).into_raw(),
            };

            let (system, history) = match decode_messages(&parsed) {
                Some(pair) => pair,
                None => return err_envelope("messages must be a JSON array").into_raw(),
            };

            // Extract last user message to drive the completion.
            // The history already contains all prior turns including the latest
            // user turn — split the last user entry off into `user_message` so
            // the backend sees a well-formed (history, new_user_msg) pair.
            let (history_without_last, user_message) = match history.split_last() {
                Some((last, rest)) if last.0 == "user" => (rest.to_vec(), last.1.clone()),
                _ => {
                    // No user turn found — pass history as-is with empty user
                    // message. This is degenerate but we degrade gracefully (D6).
                    (history, String::new())
                }
            };

            let handle_ref = unsafe { &*handle };
            let store = Arc::clone(&handle_ref.state.library.store);
            let runtime = Arc::clone(&handle_ref.state.infra.runtime);

            // Drive the full Rust tool loop (search_library, get_transcript,
            // get_podcast_info, get_memory_facts) via chat_with_tools. Swift's
            // turn-loop receives only the final prose answer — it never sees raw
            // tool_calls from this path, so Swift dispatches upgrade_thinking /
            // use_skill in its own intercept layer before calling here.
            let result = match chat_with_tools(
                &system,
                &history_without_last,
                &user_message,
                store,
                &runtime,
            ) {
                Ok(text) => text,
                Err(e) => return err_envelope(&e).into_raw(),
            };

            ok_envelope(&result).into_raw()
        },
    )
}
