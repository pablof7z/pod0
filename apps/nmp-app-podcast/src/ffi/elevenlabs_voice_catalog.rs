//! `nmp_app_podcast_elevenlabs_voice_catalog` — shared ElevenLabs voice catalog.

use std::ffi::{c_char, CString};
use std::sync::Arc;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use crate::llm::elevenlabs_voice_catalog::{self, ElevenLabsVoiceCatalogError};

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_elevenlabs_voice_catalog(
    handle: *mut PodcastHandle,
) -> *mut c_char {
    if handle.is_null() {
        return err_envelope("null handle", None, "store_unavailable").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_elevenlabs_voice_catalog",
        || err_envelope("panic", None, "panic").into_raw(),
        || {
            let handle_ref = unsafe { &*handle };
            let store = Arc::clone(&handle_ref.store);
            let runtime = Arc::clone(&handle_ref.runtime);
            match runtime.block_on(elevenlabs_voice_catalog::fetch_elevenlabs_voice_catalog(store))
            {
                Ok(result) => json_envelope(&serde_json::json!({"result": result})).into_raw(),
                Err(error) => voice_catalog_error_envelope(&error).into_raw(),
            }
        },
    )
}

fn json_envelope(value: &serde_json::Value) -> CString {
    CString::new(value.to_string())
        .unwrap_or_else(|_| CString::new(r#"{"error":{"kind":"encoding"}}"#).unwrap())
}

fn voice_catalog_error_envelope(error: &ElevenLabsVoiceCatalogError) -> CString {
    err_envelope(&error.to_string(), error.status_code(), error.kind())
}

fn err_envelope(message: &str, status_code: Option<u16>, kind: &str) -> CString {
    let mut error = serde_json::json!({
        "kind": kind,
        "message": message,
    });
    if let Some(status_code) = status_code {
        error["status_code"] = serde_json::json!(status_code);
    }
    let json = serde_json::json!({"error": error}).to_string();
    CString::new(json).unwrap_or_else(|_| CString::new(r#"{"error":{"kind":"encoding"}}"#).unwrap())
}
