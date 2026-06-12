//! Shared BYOK authorization/token exchange FFI entry points.

use std::ffi::{c_char, CStr, CString};
use std::sync::Arc;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use crate::llm::byok_auth::{self, ByokAuthError, ByokAuthorizationIntent, ByokExchangeIntent};

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_byok_authorization(intent_json: *const c_char) -> *mut c_char {
    if intent_json.is_null() {
        return err_envelope("null argument", "invalid_request").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_byok_authorization",
        || err_envelope("panic", "panic").into_raw(),
        || {
            let intent = match decode_intent::<ByokAuthorizationIntent>(intent_json) {
                Ok(intent) => intent,
                Err(error) => return err_envelope(&error, "invalid_request").into_raw(),
            };
            match byok_auth::make_authorization(intent) {
                Ok(result) => json_envelope(&serde_json::json!({"result": result})).into_raw(),
                Err(error) => byok_error_envelope(&error).into_raw(),
            }
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_byok_exchange(
    handle: *mut PodcastHandle,
    intent_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || intent_json.is_null() {
        return err_envelope("null argument", "invalid_request").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_byok_exchange",
        || err_envelope("panic", "panic").into_raw(),
        || {
            let intent = match decode_intent::<ByokExchangeIntent>(intent_json) {
                Ok(intent) => intent,
                Err(error) => return err_envelope(&error, "invalid_request").into_raw(),
            };
            let handle_ref = unsafe { &*handle };
            let runtime = Arc::clone(&handle_ref.runtime);
            match runtime.block_on(byok_auth::exchange_authorization(intent)) {
                Ok(result) => json_envelope(&serde_json::json!({"result": result})).into_raw(),
                Err(error) => byok_error_envelope(&error).into_raw(),
            }
        },
    )
}

fn decode_intent<T: serde::de::DeserializeOwned>(ptr: *const c_char) -> Result<T, String> {
    let json_str = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| "invalid UTF-8".to_owned())?;
    serde_json::from_str(json_str).map_err(|error| format!("JSON parse: {error}"))
}

fn json_envelope(value: &serde_json::Value) -> CString {
    CString::new(value.to_string())
        .unwrap_or_else(|_| CString::new(r#"{"error":{"kind":"encoding"}}"#).unwrap())
}

fn byok_error_envelope(error: &ByokAuthError) -> CString {
    err_envelope(&error.to_string(), error.kind())
}

fn err_envelope(message: &str, kind: &str) -> CString {
    let json = serde_json::json!({
        "error": {
            "kind": kind,
            "message": message,
        }
    })
    .to_string();
    CString::new(json).unwrap_or_else(|_| CString::new(r#"{"error":{"kind":"encoding"}}"#).unwrap())
}
