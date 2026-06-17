//! Feed URL normalization shared by native shells.
//!
//! Swift supplies raw user-entered text. Rust owns the product policy:
//! trimming, defaulting a missing scheme to HTTPS, allowing only HTTP(S), and
//! requiring a host before subscribe/ensure/duplicate-detection code runs.

use std::ffi::{c_char, CStr, CString};

use serde_json::json;
use url::Url;

use super::guard::ffi_guard;

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_normalize_feed_url(input: *const c_char) -> *mut c_char {
    if input.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_normalize_feed_url",
        std::ptr::null_mut,
        || {
            let raw = match unsafe { CStr::from_ptr(input) }.to_str() {
                Ok(s) => s,
                Err(_) => return encode(json!({"error": "invalid_utf8"})),
            };
            encode(match normalize_feed_url(raw) {
                Some(url) => json!({"url": url}),
                None => json!({"error": "invalid_url"}),
            })
        },
    )
}

fn normalize_feed_url(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = if has_scheme(trimmed) {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let url = Url::parse(&candidate).ok()?;
    let scheme = url.scheme().to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    if url.host_str().unwrap_or_default().is_empty() {
        return None;
    }
    Some(url.to_string())
}

fn has_scheme(value: &str) -> bool {
    let Some((first, rest)) = value.split_once(':') else {
        return false;
    };
    let mut chars = first.chars();
    let Some(first_char) = chars.next() else {
        return false;
    };
    first_char.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && !rest.is_empty()
}

fn encode(value: serde_json::Value) -> *mut c_char {
    match CString::new(value.to_string()) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}
