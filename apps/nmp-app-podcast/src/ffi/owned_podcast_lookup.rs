//! Rust-owned owned-podcast lookup projections.

use std::ffi::{c_char, CStr, CString};

use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[derive(Debug, Deserialize)]
struct OwnerPubkeyRequest {
    #[serde(default)]
    owner_pubkey: String,
}

#[derive(Debug, Serialize)]
struct OwnerPubkeyResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    podcast_id: Option<String>,
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
pub extern "C" fn nmp_app_podcast_library_podcast_for_owner_pubkey(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_library_podcast_for_owner_pubkey",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: OwnerPubkeyRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return std::ptr::null_mut(),
            };
            let owner_pubkey = request.owner_pubkey.trim();
            let handle_ref = unsafe { &*handle };
            let response = match handle_ref.state.library.store.lock() {
                Ok(store) => {
                    let podcast_id = store
                        .all_podcasts()
                        .into_iter()
                        .find(|(podcast, _)| {
                            podcast.owner_pubkey_hex.as_deref() == Some(owner_pubkey)
                        })
                        .map(|(podcast, _)| podcast.id.0.to_string());
                    OwnerPubkeyResponse { podcast_id }
                }
                Err(_) => OwnerPubkeyResponse { podcast_id: None },
            };
            encode(&response)
        },
    )
}
