//! Shared helpers for the remaining app-domain C ABI.
//!
//! Generic NMP runtime lifecycle, identity, callback, and ref APIs are exposed
//! through the app-owned UniFFI `PodcastApp` object. The C ABI kept here is the
//! string deallocator used by still-unmigrated `nmp_app_podcast_*` functions,
//! plus non-C Rust helpers shared by the TUI and UniFFI facade.

use std::ffi::{c_char, CString};

#[path = "runtime_facade_intent.rs"]
mod runtime_facade_intent;
pub use runtime_facade_intent::{
    classify_input_intent_json, decode_nip21_uri_json, dispatch_input_intent_json,
};

#[no_mangle]
pub extern "C" fn nmp_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(CString::from_raw(ptr));
        }
    }
}
