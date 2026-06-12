//! `nmp_app_podcast_speech_model_catalog` — shared speech model catalog.

use std::ffi::{c_char, CString};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use crate::llm::speech_model_catalog;

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_speech_model_catalog(handle: *mut PodcastHandle) -> *mut c_char {
    if handle.is_null() {
        return err_envelope("null handle").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_speech_model_catalog",
        || err_envelope("panic").into_raw(),
        || {
            json_envelope(
                &serde_json::json!({"result": speech_model_catalog::speech_model_catalog()}),
            )
            .into_raw()
        },
    )
}

fn json_envelope(value: &serde_json::Value) -> CString {
    CString::new(value.to_string())
        .unwrap_or_else(|_| CString::new(r#"{"error":"encoding"}"#).unwrap())
}

fn err_envelope(reason: &str) -> CString {
    let json = serde_json::json!({"error": reason}).to_string();
    CString::new(json).unwrap_or_else(|_| CString::new(r#"{"error":"encoding"}"#).unwrap())
}
