//! Rust-owned parent plan for external URL playback.
//!
//! Swift may execute the host work (create placeholder, add episode, fetch
//! feed metadata later), but Rust owns the policy for where an external audio
//! URL should be parented and whether metadata hydration should run.

use std::ffi::{c_char, CStr, CString};

use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

const UNKNOWN_PODCAST_ID: &str = "00000000-eeee-eeee-eeee-000000000000";

#[derive(Debug, Deserialize)]
struct ExternalPlayPlanRequest {
    #[serde(default)]
    feed_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct ExternalPlayPlanResponse {
    ok: bool,
    podcast_id: String,
    should_create_placeholder: bool,
    should_hydrate_metadata: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    feed_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    placeholder_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visibility: Option<&'static str>,
    title_is_placeholder: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

fn response_json(response: &ExternalPlayPlanResponse) -> *mut c_char {
    match serde_json::to_string(response) {
        Ok(json) => match CString::new(json) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

fn unknown(reason: impl Into<String>) -> ExternalPlayPlanResponse {
    ExternalPlayPlanResponse {
        ok: true,
        podcast_id: UNKNOWN_PODCAST_ID.to_owned(),
        should_create_placeholder: false,
        should_hydrate_metadata: false,
        feed_url: None,
        placeholder_title: None,
        visibility: None,
        title_is_placeholder: false,
        reason: Some(reason.into()),
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_external_play_plan(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_external_play_plan", std::ptr::null_mut, || {
        let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        let request: ExternalPlayPlanRequest = match serde_json::from_str(request_str) {
            Ok(r) => r,
            Err(_) => return std::ptr::null_mut(),
        };
        let Some(raw_feed_url) = request.feed_url.and_then(|s| {
            let trimmed = s.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }) else {
            return response_json(&unknown("no feed_url supplied"));
        };
        let Ok(feed_url) = url::Url::parse(&raw_feed_url) else {
            return response_json(&unknown("feed_url was not a valid URL"));
        };
        let scheme = feed_url.scheme().to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            return response_json(&unknown("feed_url must be http or https"));
        }
        let handle_ref = unsafe { &*handle };
        let response = match handle_ref.state.library.store.lock() {
            Ok(s) => {
                if let Some(existing) = s.podcast_by_feed_url(&feed_url) {
                    ExternalPlayPlanResponse {
                        ok: true,
                        podcast_id: existing.id.0.to_string(),
                        should_create_placeholder: false,
                        should_hydrate_metadata: false,
                        feed_url: Some(feed_url.to_string()),
                        placeholder_title: None,
                        visibility: None,
                        title_is_placeholder: false,
                        reason: Some("existing feed-backed podcast".to_owned()),
                    }
                } else {
                    ExternalPlayPlanResponse {
                        ok: true,
                        podcast_id: uuid::Uuid::new_v4().to_string(),
                        should_create_placeholder: true,
                        should_hydrate_metadata: true,
                        feed_url: Some(feed_url.to_string()),
                        placeholder_title: Some(
                            feed_url
                                .host_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| feed_url.to_string()),
                        ),
                        visibility: Some("public"),
                        title_is_placeholder: true,
                        reason: Some("new feed-backed placeholder".to_owned()),
                    }
                }
            }
            Err(_) => ExternalPlayPlanResponse {
                ok: false,
                podcast_id: UNKNOWN_PODCAST_ID.to_owned(),
                should_create_placeholder: false,
                should_hydrate_metadata: false,
                feed_url: None,
                placeholder_title: None,
                visibility: None,
                title_is_placeholder: false,
                reason: Some("store poisoned".to_owned()),
            },
        };
        response_json(&response)
    })
}
