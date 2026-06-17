//! Rust-owned Nostr identity formatting helpers for native shells.
//!
//! Native UI can display or copy identities, but NIP-19 encoding belongs with
//! the Rust identity/Nostr stack so iOS and Android cannot drift.

use std::ffi::{c_char, CStr, CString};

use serde::Serialize;

use super::guard::ffi_guard;

#[derive(Debug, Serialize)]
struct NpubResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    npub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct PubkeyResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pubkey_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    npub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'static str>,
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
pub extern "C" fn nmp_app_podcast_npub_from_hex(pubkey_hex: *const c_char) -> *mut c_char {
    if pubkey_hex.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_npub_from_hex", std::ptr::null_mut, || {
        let hex = match unsafe { CStr::from_ptr(pubkey_hex) }.to_str() {
            Ok(s) => s.trim(),
            Err(_) => {
                return encode(&NpubResponse {
                    npub: None,
                    error: Some("invalid_pubkey"),
                })
            }
        };
        let response = match nostr::PublicKey::parse(hex).and_then(|pk| pk.to_bech32()) {
            Ok(npub) => NpubResponse {
                npub: Some(npub),
                error: None,
            },
            Err(_) => NpubResponse {
                npub: None,
                error: Some("invalid_pubkey"),
            },
        };
        encode(&response)
    })
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_parse_pubkey(input: *const c_char) -> *mut c_char {
    if input.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_parse_pubkey", std::ptr::null_mut, || {
        let raw = match unsafe { CStr::from_ptr(input) }.to_str() {
            Ok(s) => s.trim(),
            Err(_) => {
                return encode(&PubkeyResponse {
                    pubkey_hex: None,
                    npub: None,
                    error: Some("invalid_pubkey"),
                })
            }
        };
        let response = match nostr::PublicKey::parse(raw) {
            Ok(pubkey) => PubkeyResponse {
                pubkey_hex: Some(pubkey.to_hex()),
                npub: pubkey.to_bech32().ok(),
                error: None,
            },
            Err(_) => PubkeyResponse {
                pubkey_hex: None,
                npub: None,
                error: Some("invalid_pubkey"),
            },
        };
        encode(&response)
    })
}
