//! `nmp_app_podcast_generate_image` — synchronous provider image generation.
//!
//! Swift keeps platform-specific blob upload and file handling, but provider
//! HTTP routing, request bodies, and response extraction live in Rust.

use std::ffi::{c_char, CStr, CString};
use std::sync::Arc;

use base64::Engine;
use serde::Deserialize;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use crate::llm::image_generation::{generate_openrouter_image, ImageGenerationRequest};

#[derive(Deserialize)]
struct GenerateImageInput {
    prompt: String,
    #[serde(default)]
    model: Option<String>,
}

fn ok_envelope(bytes: &[u8]) -> CString {
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let json = serde_json::json!({ "image_base64": b64 }).to_string();
    CString::new(json).unwrap_or_else(|_| CString::new(r#"{"error":"encoding"}"#).unwrap())
}

fn err_envelope(reason: &str) -> CString {
    let json = serde_json::json!({ "error": reason }).to_string();
    CString::new(json).unwrap_or_else(|_| CString::new(r#"{"error":"encoding"}"#).unwrap())
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_generate_image(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return err_envelope("null argument").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_generate_image",
        || err_envelope("panic").into_raw(),
        || {
            let json_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return err_envelope("invalid UTF-8").into_raw(),
            };
            let input: GenerateImageInput = match serde_json::from_str(json_str) {
                Ok(v) => v,
                Err(e) => return err_envelope(&format!("JSON parse: {e}")).into_raw(),
            };
            if input.prompt.is_empty() {
                return err_envelope("prompt is empty").into_raw();
            }

            let handle_ref = unsafe { &*handle };
            let request = ImageGenerationRequest {
                prompt: input.prompt,
                model: input.model,
            };
            match generate_openrouter_image(Arc::clone(&handle_ref.store), &request) {
                Ok(image) => ok_envelope(&image.bytes).into_raw(),
                Err(e) => err_envelope(&e.to_string()).into_raw(),
            }
        },
    )
}
