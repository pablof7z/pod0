//! `nmp_app_podcast_provider_embed` — shared provider embeddings.

use std::ffi::{c_char, CStr, CString};
use std::sync::Arc;

use super::handle::PodcastHandle;
use crate::llm::provider_transport::{self, EmbeddingIntent};

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_provider_embed(
    handle: *mut PodcastHandle,
    intent_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || intent_json.is_null() {
        return err_envelope("null argument").into_raw();
    }
    let json_str = match unsafe { CStr::from_ptr(intent_json) }.to_str() {
        Ok(s) => s,
        Err(_) => return err_envelope("invalid UTF-8").into_raw(),
    };
    let intent: EmbeddingIntent = match serde_json::from_str(json_str) {
        Ok(intent) => intent,
        Err(e) => return err_envelope(&format!("JSON parse: {e}")).into_raw(),
    };
    let handle_ref = unsafe { &*handle };
    let store = Arc::clone(&handle_ref.store);
    let runtime = Arc::clone(&handle_ref.runtime);
    match runtime.block_on(provider_transport::embed(store, intent)) {
        Ok(result) => json_envelope(&serde_json::json!({"result": result})).into_raw(),
        Err(error) => err_envelope(&error.to_string()).into_raw(),
    }
}

fn json_envelope(value: &serde_json::Value) -> CString {
    CString::new(value.to_string())
        .unwrap_or_else(|_| CString::new(r#"{"error":"encoding"}"#).unwrap())
}

fn err_envelope(reason: &str) -> CString {
    let json = serde_json::json!({"error": reason}).to_string();
    CString::new(json).unwrap_or_else(|_| CString::new(r#"{"error":"encoding"}"#).unwrap())
}
