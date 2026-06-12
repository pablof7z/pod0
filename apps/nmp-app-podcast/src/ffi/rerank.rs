//! `nmp_app_podcast_rerank` — synchronous RAG reranker FFI.
//!
//! Swift sends a provider-neutral rerank request and receives either sorted
//! document indices or a typed error envelope. OpenRouter URL/header/body
//! shaping stays in Rust.

use std::ffi::{c_char, CStr, CString};
use std::sync::Arc;

use serde::Serialize;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use crate::llm::{rerank_openrouter, RerankError, RerankRequest};

#[derive(Serialize)]
struct RerankOkEnvelope {
    indices: Vec<usize>,
}

#[derive(Serialize)]
struct RerankErrorEnvelope<'a> {
    error: RerankErrorBody<'a>,
}

#[derive(Serialize)]
struct RerankErrorBody<'a> {
    kind: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    status_code: Option<u16>,
}

fn cstring_json<T: Serialize>(value: &T) -> CString {
    let json = serde_json::to_string(value).unwrap_or_else(|_| {
        r#"{"error":{"kind":"encoding","message":"failed to encode response"}}"#.to_owned()
    });
    CString::new(json).unwrap_or_else(|_| {
        CString::new(r#"{"error":{"kind":"encoding","message":"nul byte in response"}}"#)
            .expect("static string has no nul")
    })
}

fn ok_envelope(indices: Vec<usize>) -> CString {
    cstring_json(&RerankOkEnvelope { indices })
}

fn err_envelope(error: &RerankError) -> CString {
    cstring_json(&RerankErrorEnvelope {
        error: RerankErrorBody {
            kind: error.kind(),
            message: error.message(),
            status_code: error.status_code(),
        },
    })
}

fn static_error(kind: &'static str, message: &'static str) -> CString {
    cstring_json(&RerankErrorEnvelope {
        error: RerankErrorBody {
            kind,
            message,
            status_code: None,
        },
    })
}

/// Rerank documents through the Rust-owned provider transport.
///
/// `request_json` shape:
/// `{"model":"cohere/rerank-v3.5","query":"...","documents":["..."],"top_n":10}`
///
/// Returns:
/// `{"indices":[0,2,1]}` or
/// `{"error":{"kind":"missing_api_key","message":"..."}}`.
/// Caller MUST free the pointer via `nmp_app_free_string`.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_rerank(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return static_error("invalid_request", "null argument").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_rerank",
        || static_error("panic", "panic in ffi").into_raw(),
        || {
            let json_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => {
                    return static_error("invalid_request", "invalid UTF-8").into_raw()
                }
            };

            let request: RerankRequest = match serde_json::from_str(json_str) {
                Ok(request) => request,
                Err(e) => {
                    let error = RerankError::InvalidRequest(format!("JSON parse: {e}"));
                    return err_envelope(&error).into_raw();
                }
            };

            let handle_ref = unsafe { &*handle };
            let store = Arc::clone(&handle_ref.store);
            let api_key = match store.lock() {
                Ok(store) => store.open_router_api_key().map(str::to_owned),
                Err(_) => {
                    return static_error("transport", "settings store unavailable").into_raw();
                }
            };

            match rerank_openrouter(api_key, request) {
                Ok(indices) => ok_envelope(indices).into_raw(),
                Err(error) => err_envelope(&error).into_raw(),
            }
        },
    )
}
