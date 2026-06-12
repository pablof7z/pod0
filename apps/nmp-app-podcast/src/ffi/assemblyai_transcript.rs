//! `nmp_app_podcast_assemblyai_transcribe` - shared AssemblyAI STT.

use std::ffi::{c_char, CStr, CString};
use std::sync::Arc;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use crate::llm::assemblyai_transcript::{
    self, AssemblyAITranscriptError, AssemblyAITranscriptIntent,
};

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_assemblyai_transcribe(
    handle: *mut PodcastHandle,
    intent_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || intent_json.is_null() {
        return err_envelope("null argument", None, "invalid_request").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_assemblyai_transcribe",
        || err_envelope("panic", None, "panic").into_raw(),
        || {
            let json_str = match unsafe { CStr::from_ptr(intent_json) }.to_str() {
                Ok(s) => s,
                Err(_) => {
                    return err_envelope("invalid UTF-8", None, "invalid_request").into_raw()
                }
            };
            let intent: AssemblyAITranscriptIntent = match serde_json::from_str(json_str) {
                Ok(intent) => intent,
                Err(e) => {
                    return err_envelope(&format!("JSON parse: {e}"), None, "invalid_request")
                        .into_raw()
                }
            };
            let handle_ref = unsafe { &*handle };
            let store = Arc::clone(&handle_ref.store);
            let runtime = Arc::clone(&handle_ref.runtime);
            match runtime.block_on(assemblyai_transcript::transcribe_assemblyai(store, intent)) {
                Ok(result) => json_envelope(&serde_json::json!({"result": result})).into_raw(),
                Err(error) => assemblyai_error_envelope(&error).into_raw(),
            }
        },
    )
}

fn json_envelope(value: &serde_json::Value) -> CString {
    CString::new(value.to_string())
        .unwrap_or_else(|_| CString::new(r#"{"error":{"kind":"encoding"}}"#).unwrap())
}

fn assemblyai_error_envelope(error: &AssemblyAITranscriptError) -> CString {
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
