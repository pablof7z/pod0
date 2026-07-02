//! Local on-device LLM backend implementation via callback socket.
//!
//! This backend delegates to a local LiteRT-LM or similar on-device model
//! registered from the iOS side. The Swift side registers a callback (context pointer
//! + callback function) globally; Rust calls it with a JSON prompt and receives
//! a JSON response back.

use async_trait::async_trait;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::{Mutex, OnceLock};

use super::backend::{LlmBackend, LlmError, LlmRequest};

/// Release a heap-allocated C string returned by `reg.callback`.
///
/// The callback trampoline allocates its response via `CString::into_raw`
/// (matching the C-ABI convention this module documents at the type alias
/// above), so the memory belongs to the Rust allocator and must come back
/// through it — not the host's `free(3)`. This used to be the shared
/// `nmp_ffi::nmp_free_string` symbol; that crate is deleted, and this
/// callback is podcast's own local bridging concern (not an NMP FFI
/// surface), so the free path is inlined here rather than reaching for a
/// framework helper. Passing `NULL` is a no-op.
unsafe fn free_local_llm_response(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    drop(CString::from_raw(ptr));
}

/// FFI callback function type: takes context pointer and JSON prompt (C string),
/// returns JSON response (malloc-compatible C string that Rust must free).
pub type NmpLocalLlmFn =
    extern "C" fn(*mut c_void, *const std::ffi::c_char) -> *mut std::ffi::c_char;

/// Global registration for the local LLM callback.
/// The context is a usize-encoded Unmanaged<LocalLLMService> pointer owned by Swift
/// for the app lifetime (D6: SAFETY is caller's responsibility).
pub(crate) struct LocalLlmRegistration {
    pub context: usize,
    pub callback: NmpLocalLlmFn,
}

// SAFETY: context is a usize-encoded Unmanaged pointer, owned by Swift for the app lifetime.
// Only called from Rust when explicitly registered; no data races.
unsafe impl Send for LocalLlmRegistration {}

/// Global callback socket (OnceLock for init-once semantics).
static LOCAL_LLM: OnceLock<Mutex<Option<LocalLlmRegistration>>> = OnceLock::new();

/// Return the global local LLM registration slot.
pub(crate) fn slot() -> &'static Mutex<Option<LocalLlmRegistration>> {
    LOCAL_LLM.get_or_init(|| Mutex::new(None))
}

/// Register or clear the global local LLM callback.
pub(crate) fn set_registration(reg: Option<LocalLlmRegistration>) {
    if let Ok(mut slot_guard) = slot().lock() {
        *slot_guard = reg;
    }
}

/// Local on-device LLM backend.
///
/// Holds the model ID and delegates to the registered global callback when `complete` is called.
/// If the callback slot is empty (not registered), returns `Unavailable`.
pub struct LocalModelBackend {
    pub model_id: String,
}

#[async_trait]
impl LlmBackend for LocalModelBackend {
    async fn complete(&self, req: &LlmRequest) -> Result<String, LlmError> {
        // Build the prompt JSON exactly per spec:
        // {"system":..,"history":[[role,content],..],"user":..,"model":self.model_id}
        let prompt_json = serde_json::json!({
            "system": req.system,
            "history": req.history.iter().map(|(role, content)| {
                vec![serde_json::Value::String(role.clone()), serde_json::Value::String(content.clone())]
            }).collect::<Vec<_>>(),
            "user": req.user,
            "model": self.model_id,
        });

        let prompt_json_str = prompt_json.to_string();
        let prompt_cstring = match CString::new(prompt_json_str) {
            Ok(s) => s,
            Err(_) => return Err(LlmError::Unavailable("Failed to encode prompt JSON".into())),
        };

        // Lock the slot and check if a callback is registered.
        let slot_guard = match slot().lock() {
            Ok(guard) => guard,
            Err(_) => {
                return Err(LlmError::Unavailable(
                    "Failed to acquire callback slot lock".into(),
                ))
            }
        };

        let reg = match &*slot_guard {
            Some(r) => r,
            None => return Err(LlmError::Unavailable("Local model not loaded".into())),
        };

        // Call the FFI callback: (context as *mut c_void, prompt.as_ptr())
        let response_ptr = (reg.callback)(reg.context as *mut c_void, prompt_cstring.as_ptr());

        // Check for null response (error case).
        if response_ptr.is_null() {
            return Err(LlmError::Unavailable(
                "Local model call returned null".into(),
            ));
        }

        // Convert the returned C string to Rust String.
        let response_cstr = match unsafe { CStr::from_ptr(response_ptr) }.to_str() {
            Ok(s) => s.to_owned(),
            Err(_) => {
                // Free the pointer before returning error.
                unsafe { free_local_llm_response(response_ptr) };
                return Err(LlmError::Unavailable(
                    "Local model response not valid UTF-8".into(),
                ));
            }
        };

        // Free the returned C string via the Rust helper.
        unsafe { free_local_llm_response(response_ptr) };

        // Parse the response JSON: {"text":..} or {"error":..}
        match serde_json::from_str::<serde_json::Value>(&response_cstr) {
            Ok(json) => {
                if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
                    Ok(text.to_string())
                } else if let Some(error) = json.get("error").and_then(|v| v.as_str()) {
                    // Treat model errors as Unavailable (model-not-loaded is unavailable, not provider error).
                    Err(LlmError::Unavailable(error.to_string()))
                } else {
                    Err(LlmError::Unavailable(
                        "Local model response missing 'text' or 'error'".into(),
                    ))
                }
            }
            Err(_) => Err(LlmError::Unavailable(
                "Failed to parse local model response JSON".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_backend_unavailable_when_not_registered() {
        let backend = LocalModelBackend {
            model_id: "test-model".to_string(),
        };

        let req = LlmRequest {
            system: "You are helpful.".to_string(),
            history: vec![],
            user: "Hello".to_string(),
            model: "unused".to_string(),
        };

        let result = backend.complete(&req).await;
        assert!(result.is_err());
        if let Err(LlmError::Unavailable(msg)) = result {
            assert!(msg.contains("Local model not loaded"));
        } else {
            panic!("Expected Unavailable error");
        }
    }
}
