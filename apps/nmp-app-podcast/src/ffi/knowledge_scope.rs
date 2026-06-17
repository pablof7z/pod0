//! Rust-owned knowledge scope resolution.
//!
//! Agent transcript search can receive a raw UUID-like scope from tool calls.
//! Rust owns deciding whether that reference is a canonical episode id or a
//! podcast id before the knowledge query is scoped.

use std::ffi::{c_char, CStr, CString};

use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[derive(Debug, Deserialize)]
struct KnowledgeScopeRequest {
    #[serde(default)]
    scope: String,
}

#[derive(Debug, Serialize)]
struct KnowledgeScopeResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    podcast_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    episode_id: Option<String>,
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
pub extern "C" fn nmp_app_podcast_knowledge_resolve_scope(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_knowledge_resolve_scope",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: KnowledgeScopeRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return std::ptr::null_mut(),
            };
            let raw = request.scope.trim();
            if raw.is_empty() {
                return encode(&KnowledgeScopeResponse {
                    podcast_id: None,
                    episode_id: None,
                });
            }
            let raw_lower = raw.to_lowercase();
            let handle_ref = unsafe { &*handle };
            let response = match handle_ref.state.library.store.lock() {
                Ok(store) => {
                    if store.has_episode(&raw_lower) {
                        KnowledgeScopeResponse {
                            podcast_id: None,
                            episode_id: Some(raw_lower),
                        }
                    } else if store.podcast_by_id_str(&raw_lower).is_some() {
                        KnowledgeScopeResponse {
                            podcast_id: Some(raw_lower),
                            episode_id: None,
                        }
                    } else {
                        KnowledgeScopeResponse {
                            podcast_id: None,
                            episode_id: Some(raw_lower),
                        }
                    }
                }
                Err(_) => KnowledgeScopeResponse {
                    podcast_id: None,
                    episode_id: Some(raw_lower),
                },
            };
            encode(&response)
        },
    )
}
