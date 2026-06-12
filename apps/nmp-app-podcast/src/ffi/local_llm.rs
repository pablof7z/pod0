//! `nmp_app_register_local_llm` and `nmp_app_clear_local_llm` — FFI socket for local LLM callback.
//!
//! The iOS `LocalLLMService` registers and clears the global callback for on-device
//! LiteRT-LM or similar inference. The callback is held in a global static-lifetime slot
//! (OnceLock<Mutex<Option<LocalLlmRegistration>>>) and called synchronously from Rust
//! when the `LocalModelBackend` is active.

use std::ffi::c_void;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use crate::llm::local_model_backend::{set_registration, LocalLlmRegistration, NmpLocalLlmFn};

/// Register the local LLM callback globally.
///
/// ## Arguments
/// - `handle`: PodcastHandle (accepted for API symmetry; registration is global).
/// - `context`: usize-encoded Unmanaged<LocalLLMService> pointer (owned by Swift for app lifetime).
/// - `fn`: FFI callback function (takes context + JSON prompt, returns malloc-compatible JSON response).
///
/// ## Thread safety
/// This can be called from any thread (typically iOS main thread) and synchronously
/// updates the global OnceLock<Mutex<Option<...>>>.
///
/// ## D6: D6 silent no-ops
/// Null handle is silently ignored (registration is global, handle param is API-symmetry only).
#[no_mangle]
pub extern "C" fn nmp_app_register_local_llm(
    _handle: *mut PodcastHandle,
    context: *mut c_void,
    callback: NmpLocalLlmFn,
) {
    ffi_guard("nmp_app_register_local_llm", || (), || {
        // Register the callback globally.
        set_registration(Some(LocalLlmRegistration {
            context: context as usize,
            callback,
        }));
    });
}

/// Clear the local LLM callback globally.
///
/// ## Arguments
/// - `handle`: PodcastHandle (accepted for API symmetry; registration is global).
///
/// ## D6: D6 silent no-ops
/// Null handle is silently ignored (registration is global, handle param is API-symmetry only).
#[no_mangle]
pub extern "C" fn nmp_app_clear_local_llm(_handle: *mut PodcastHandle) {
    ffi_guard("nmp_app_clear_local_llm", || (), || {
        // Clear the callback globally.
        set_registration(None);
    });
}
