//! Rust-owned `record_memory` helper for freeform agent memories.
//!
//! The agent tool supplies only text. Rust owns the canonical identity/key
//! minted for that text and writes the durable `MemoryFact` store.

use std::ffi::{c_char, CStr, CString};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[derive(Debug, Deserialize)]
struct RememberTextRequest {
    content: String,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Serialize)]
struct RememberTextResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

fn response_json(response: &RememberTextResponse) -> *mut c_char {
    match serde_json::to_string(response) {
        Ok(json) => match CString::new(json) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

fn parse_request(request_json: *const c_char) -> Option<RememberTextRequest> {
    if request_json.is_null() {
        return None;
    }
    let request_str = unsafe { CStr::from_ptr(request_json) }.to_str().ok()?;
    serde_json::from_str(request_str).ok()
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_memory_remember_text(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_memory_remember_text",
        std::ptr::null_mut,
        || {
            let Some(request) = parse_request(request_json) else {
                return std::ptr::null_mut();
            };
            let value = request.content.trim();
            if value.is_empty() {
                return response_json(&RememberTextResponse {
                    ok: false,
                    id: None,
                    key: None,
                    value: None,
                    source: None,
                    message: Some("missing memory content".to_owned()),
                });
            }
            let key = format!("agent_{}", uuid::Uuid::new_v4().simple());
            let source = request
                .source
                .and_then(|s| {
                    let trimmed = s.trim();
                    (!trimmed.is_empty()).then(|| trimmed.to_owned())
                })
                .unwrap_or_else(|| "agent".to_owned());
            let handle_ref = unsafe { &*handle };
            let response = match handle_ref.state.library.store.lock() {
                Ok(mut store) => {
                    store.set_memory_fact(
                        key.clone(),
                        value.to_owned(),
                        source.clone(),
                        Utc::now().timestamp(),
                    );
                    RememberTextResponse {
                        ok: true,
                        id: Some(key.clone()),
                        key: Some(key),
                        value: Some(value.to_owned()),
                        source: Some(source),
                        message: Some("Saved memory".to_owned()),
                    }
                }
                Err(_) => RememberTextResponse {
                    ok: false,
                    id: None,
                    key: None,
                    value: None,
                    source: None,
                    message: Some("memory store unavailable".to_owned()),
                },
            };
            if response.ok {
                handle_ref.bump_snapshot_rev_domain(crate::state::Domain::Misc);
            }
            response_json(&response)
        },
    )
}
