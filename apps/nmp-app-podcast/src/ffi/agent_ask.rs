//! Rust-owned policy for the agent `ask` tool.
//!
//! Swift presents the current pending question and reports raw owner actions
//! (answer / decline / timeout). This module owns normalization, FIFO
//! promotion, timeout duration, and the final tool-result envelope.

use std::collections::VecDeque;
use std::ffi::{c_char, CStr, CString};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

const TIMEOUT_SECONDS: u64 = 5 * 60;
const DECLINED_ANSWER: &str = "user declined to answer";
const TIMED_OUT_ANSWER: &str = "user did not respond within 5 minutes";

pub(crate) type AgentAskCallback = Arc<dyn Fn(String) + Send + Sync + 'static>;

#[derive(Clone, Default)]
pub(crate) struct AgentAskCallbackState {
    callback: Option<AgentAskCallback>,
}

impl AgentAskCallbackState {
    fn emit(&self, response: &AgentAskResponse) {
        let Some(callback) = self.callback.as_ref().cloned() else {
            return;
        };
        let Ok(json) = serde_json::to_string(response) else {
            return;
        };
        callback(json);
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AgentAskPending {
    id: String,
    question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
    created_at: i64,
    timeout_seconds: u64,
}

#[derive(Debug, Default)]
pub(crate) struct AgentAskState {
    queue: VecDeque<AgentAskPending>,
}

#[derive(Debug, Deserialize)]
struct AgentAskEnqueueRequest {
    question: String,
    #[serde(default)]
    context: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AgentAskSettleRequest {
    id: String,
    outcome: String,
    #[serde(default)]
    answer: Option<String>,
}

#[derive(Debug, Serialize)]
struct AgentAskResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    current: Option<AgentAskPending>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enqueued: Option<AgentAskPending>,
    #[serde(skip_serializing_if = "Option::is_none")]
    settled_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl AgentAskState {
    fn enqueue(&mut self, request: AgentAskEnqueueRequest) -> AgentAskResponse {
        let question = request.question.trim();
        if question.is_empty() {
            return AgentAskResponse {
                ok: false,
                current: self.current(),
                enqueued: None,
                settled_id: None,
                result: Some(tool_error("Missing or empty 'question'")),
                message: Some("missing question".to_owned()),
            };
        }
        let context = request.context.and_then(|raw| {
            let trimmed = raw.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        });
        let pending = AgentAskPending {
            id: Uuid::new_v4().to_string(),
            question: question.to_owned(),
            context,
            created_at: Utc::now().timestamp(),
            timeout_seconds: TIMEOUT_SECONDS,
        };
        self.queue.push_back(pending.clone());
        AgentAskResponse {
            ok: true,
            current: self.current(),
            enqueued: Some(pending),
            settled_id: None,
            result: None,
            message: None,
        }
    }

    fn settle(&mut self, request: AgentAskSettleRequest) -> AgentAskResponse {
        self.settle_internal(request.id, request.outcome, request.answer)
    }

    fn settle_timeout(&mut self, id: String) -> AgentAskResponse {
        self.settle_internal(id, "timeout".to_owned(), None)
    }

    fn settle_internal(
        &mut self,
        id: String,
        outcome: String,
        answer: Option<String>,
    ) -> AgentAskResponse {
        let Some(index) = self.queue.iter().position(|ask| ask.id == id) else {
            return AgentAskResponse {
                ok: false,
                current: self.current(),
                enqueued: None,
                settled_id: None,
                result: None,
                message: Some("ask already settled".to_owned()),
            };
        };
        self.queue.remove(index);
        let outcome = outcome.trim();
        let result = match outcome {
            "answer" => {
                let answer = answer.unwrap_or_default();
                let trimmed = answer.trim();
                if trimmed.is_empty() {
                    tool_error("Missing or empty ask answer")
                } else {
                    tool_success_answer(trimmed)
                }
            }
            "decline" => tool_success_answer(DECLINED_ANSWER),
            "timeout" => tool_success_answer(TIMED_OUT_ANSWER),
            _ => tool_error("Unknown ask settlement outcome"),
        };
        AgentAskResponse {
            ok: true,
            current: self.current(),
            enqueued: None,
            settled_id: Some(id),
            result: Some(result),
            message: None,
        }
    }

    fn current(&self) -> Option<AgentAskPending> {
        self.queue.front().cloned()
    }
}

fn spawn_timeout(
    handle: &PodcastHandle,
    pending: &AgentAskPending,
    callback_state: Arc<Mutex<AgentAskCallbackState>>,
) {
    let ask_state = handle.ask_state.clone();
    let id = pending.id.clone();
    handle.state.infra.runtime.spawn(async move {
        tokio::time::sleep(Duration::from_secs(TIMEOUT_SECONDS)).await;
        let response = match ask_state.lock() {
            Ok(mut state) => state.settle_timeout(id),
            Err(_) => AgentAskResponse {
                ok: false,
                current: None,
                enqueued: None,
                settled_id: None,
                result: Some(tool_error("ask queue unavailable")),
                message: Some("ask queue poisoned".to_owned()),
            },
        };
        if response.ok && response.result.is_some() {
            if let Ok(callback_state) = callback_state.lock() {
                callback_state.emit(&response);
            }
        }
    });
}

fn tool_success_answer(answer: &str) -> String {
    serde_json::to_string(&json!({"success": true, "answer": answer}))
        .unwrap_or_else(|_| "{\"success\":true}".to_owned())
}

fn tool_error(message: &str) -> String {
    serde_json::to_string(&json!({"error": message}))
        .unwrap_or_else(|_| "{\"error\":\"unknown\"}".to_owned())
}

fn response_json(response: &AgentAskResponse) -> *mut c_char {
    match serde_json::to_string(response) {
        Ok(json) => match CString::new(json) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

fn parse_request<T: for<'de> Deserialize<'de>>(request_json: *const c_char) -> Option<T> {
    if request_json.is_null() {
        return None;
    }
    let request_str = unsafe { CStr::from_ptr(request_json) }.to_str().ok()?;
    serde_json::from_str(request_str).ok()
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_ask_enqueue(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_ask_enqueue",
        std::ptr::null_mut,
        || {
            let Some(request) = parse_request::<AgentAskEnqueueRequest>(request_json) else {
                return std::ptr::null_mut();
            };
            let handle_ref = unsafe { &*handle };
            let response = match handle_ref.ask_state.lock() {
                Ok(mut state) => state.enqueue(request),
                Err(_) => AgentAskResponse {
                    ok: false,
                    current: None,
                    enqueued: None,
                    settled_id: None,
                    result: Some(tool_error("ask queue unavailable")),
                    message: Some("ask queue poisoned".to_owned()),
                },
            };
            if let Some(pending) = response.enqueued.as_ref() {
                spawn_timeout(handle_ref, pending, handle_ref.ask_callback.clone());
            }
            response_json(&response)
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_ask_settle(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_ask_settle",
        std::ptr::null_mut,
        || {
            let Some(request) = parse_request::<AgentAskSettleRequest>(request_json) else {
                return std::ptr::null_mut();
            };
            let handle_ref = unsafe { &*handle };
            let response = match handle_ref.ask_state.lock() {
                Ok(mut state) => state.settle(request),
                Err(_) => AgentAskResponse {
                    ok: false,
                    current: None,
                    enqueued: None,
                    settled_id: None,
                    result: Some(tool_error("ask queue unavailable")),
                    message: Some("ask queue poisoned".to_owned()),
                },
            };
            response_json(&response)
        },
    )
}

pub(crate) fn set_agent_ask_callback(handle: &PodcastHandle, callback: Option<AgentAskCallback>) {
    if let Ok(mut slot) = handle.ask_callback.lock() {
        *slot = AgentAskCallbackState { callback };
    }
}
