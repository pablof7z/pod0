//! Rust-owned agent chat title prompt and response parsing.
//!
//! Swift executes the async provider call as a capability. Rust owns which
//! messages matter, transcript truncation, prompt text, title constraints, and
//! response parsing.

use std::ffi::{c_char, CStr, CString};

use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

const MAX_TRANSCRIPT_CHARS: usize = 4_000;
const MAX_TITLE_CHARS: usize = 60;

#[derive(Debug, Deserialize)]
struct TitlePromptRequest {
    #[serde(default)]
    messages: Vec<TitleMessage>,
}

#[derive(Debug, Deserialize)]
struct TitleMessage {
    role: String,
    text: String,
}

#[derive(Debug, Serialize)]
struct TitlePromptResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TitleParseRequest {
    raw_content: String,
}

#[derive(Debug, Deserialize)]
struct ModelTitleResponse {
    title: String,
}

#[derive(Debug, Serialize)]
struct TitleParseResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
}

fn encode<T: Serialize>(value: &T) -> *mut c_char {
    match serde_json::to_string(value) {
        Ok(json) => match CString::new(json) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_chat_title_prompt(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_chat_title_prompt",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: TitlePromptRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(&prompt_error("invalid_request")),
            };
            let handle_ref = unsafe { &*handle };
            let response = match handle_ref.state.library.store.lock() {
                Ok(store) => build_prompt_response(&store, request.messages),
                Err(_) => prompt_error("store_unavailable"),
            };
            encode(&response)
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_agent_chat_title_parse(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_agent_chat_title_parse",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: TitleParseRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return encode(&parse_error("invalid_request")),
            };
            encode(&parse_response(&request.raw_content))
        },
    )
}

fn build_prompt_response(
    store: &crate::store::PodcastStore,
    messages: Vec<TitleMessage>,
) -> TitlePromptResponse {
    let model = store.memory_compilation_model().trim().to_string();
    if model.is_empty() {
        return prompt_error("no_model_selected");
    }
    let transcript = transcript_snippet(messages);
    if transcript.is_empty() {
        return TitlePromptResponse {
            error: Some("empty_transcript".to_string()),
            model: Some(model),
            system_prompt: None,
            user_prompt: None,
        };
    }
    TitlePromptResponse {
        error: None,
        model: Some(model),
        system_prompt: Some(system_prompt()),
        user_prompt: Some(user_prompt(&transcript)),
    }
}

fn transcript_snippet(messages: Vec<TitleMessage>) -> String {
    let mut lines = Vec::new();
    for message in messages {
        let text = sanitize(&message.text);
        if text.is_empty() {
            continue;
        }
        match message.role.as_str() {
            "user" => lines.push(format!("User: {text}")),
            "assistant" => lines.push(format!("Assistant: {text}")),
            _ => {}
        }
    }
    let joined = lines.join("\n");
    if joined.chars().count() <= MAX_TRANSCRIPT_CHARS {
        return joined;
    }
    joined.chars().take(MAX_TRANSCRIPT_CHARS).collect()
}

fn system_prompt() -> String {
    "You write very short titles for chat-conversation history lists. Reply strictly with JSON of the form {\"title\": String}. The title must be 2 to 6 words, no punctuation at the end, no quotation marks, no emoji, and must describe the actual subject of the conversation (not \"Chat\" or \"Untitled\").".to_string()
}

fn user_prompt(transcript: &str) -> String {
    format!(
        "Generate a short title that summarises what this conversation is about.\n\nTranscript:\n{transcript}"
    )
}

fn parse_response(raw_content: &str) -> TitleParseResponse {
    let decoded: ModelTitleResponse = match serde_json::from_str(&extract_json(raw_content)) {
        Ok(value) => value,
        Err(_) => return parse_error("invalid_response"),
    };
    let mut title = decoded
        .title
        .trim()
        .trim_matches(|ch: char| "\"'.,;:!?".contains(ch))
        .trim()
        .to_string();
    title = sanitize(&title);
    if title.is_empty() || title.eq_ignore_ascii_case("chat") || title.eq_ignore_ascii_case("untitled") {
        return parse_error("invalid_response");
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        title = title.chars().take(MAX_TITLE_CHARS).collect();
    }
    TitleParseResponse {
        error: None,
        title: Some(title),
    }
}

fn prompt_error(error: &str) -> TitlePromptResponse {
    TitlePromptResponse {
        error: Some(error.to_string()),
        model: None,
        system_prompt: None,
        user_prompt: None,
    }
}

fn parse_error(error: &str) -> TitleParseResponse {
    TitleParseResponse {
        error: Some(error.to_string()),
        title: None,
    }
}

fn sanitize(value: &str) -> String {
    value
        .replace(['\r', '\n'], " ")
        .chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

fn extract_json(content: &str) -> String {
    fenced_substring(content, Some("json"))
        .or_else(|| fenced_substring(content, None))
        .unwrap_or_else(|| content.trim().to_string())
}

fn fenced_substring(content: &str, language: Option<&str>) -> Option<String> {
    let marker = language
        .map(|lang| format!("```{lang}"))
        .unwrap_or_else(|| "```".to_string());
    let start = content.find(&marker)? + marker.len();
    let after_open = &content[start..];
    let newline = after_open.find('\n')?;
    let body_start = start + newline + 1;
    let close = content[body_start..].find("```")? + body_start;
    Some(content[body_start..close].trim().to_string())
}
