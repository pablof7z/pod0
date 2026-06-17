//! Rust-owned Apple Podcasts directory search and lookup FFI.
//!
//! Swift supplies only user intent (`query`, `type`, `limit`, collection id)
//! and executes the raw HTTP capability. Rust owns endpoint shape, limits,
//! response parsing, and error envelopes.

use std::ffi::{c_char, CStr, CString};
use std::sync::Arc;

use podcast_feeds::http::{HttpRequest, HttpResult};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::itunes::{self, ItunesSearchKind};

#[derive(serde::Deserialize)]
struct SearchIntent {
    query: String,
    #[serde(rename = "type", default = "default_search_type")]
    search_type: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(serde::Deserialize)]
struct LookupIntent {
    collection_id: String,
}

#[derive(serde::Deserialize)]
struct TopPodcastsIntent {
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default = "default_storefront")]
    storefront: String,
}

fn default_search_type() -> String {
    "episode".to_owned()
}

fn default_limit() -> usize {
    5
}

fn default_storefront() -> String {
    "us".to_owned()
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_itunes_directory_search(
    handle: *mut PodcastHandle,
    intent_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || intent_json.is_null() {
        return err_envelope("null argument").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_itunes_directory_search",
        || err_envelope("panic").into_raw(),
        || {
            let json_str = match unsafe { CStr::from_ptr(intent_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return err_envelope("invalid UTF-8").into_raw(),
            };
            let intent: SearchIntent = match serde_json::from_str(json_str) {
                Ok(intent) => intent,
                Err(e) => return err_envelope(&format!("JSON parse: {e}")).into_raw(),
            };
            let query = intent.query.trim();
            if query.is_empty() {
                return json_envelope(&serde_json::json!({"result": []})).into_raw();
            }
            let kind = match ItunesSearchKind::from_str(intent.search_type.as_str()) {
                Some(kind) => kind,
                None => return err_envelope("invalid directory search type").into_raw(),
            };
            let url = itunes::search_url(query, kind, intent.limit);
            match fetch_body(handle, url, "itunes-directory-search") {
                Ok(body) => {
                    let result = itunes::parse_itunes_directory_results(&body, kind);
                    json_envelope(&serde_json::json!({"result": result})).into_raw()
                }
                Err(error) => err_envelope(&error).into_raw(),
            }
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_itunes_lookup_feed_url(
    handle: *mut PodcastHandle,
    intent_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || intent_json.is_null() {
        return err_envelope("null argument").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_itunes_lookup_feed_url",
        || err_envelope("panic").into_raw(),
        || {
            let json_str = match unsafe { CStr::from_ptr(intent_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return err_envelope("invalid UTF-8").into_raw(),
            };
            let intent: LookupIntent = match serde_json::from_str(json_str) {
                Ok(intent) => intent,
                Err(e) => return err_envelope(&format!("JSON parse: {e}")).into_raw(),
            };
            let Some(url) = itunes::lookup_url(&intent.collection_id) else {
                return json_envelope(&serde_json::json!({"feed_url": null})).into_raw();
            };
            match fetch_body(handle, url, "itunes-lookup-feed") {
                Ok(body) => {
                    let feed_url = itunes::parse_lookup_feed_url(&body);
                    json_envelope(&serde_json::json!({"feed_url": feed_url})).into_raw()
                }
                Err(error) => err_envelope(&error).into_raw(),
            }
        },
    )
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_itunes_top_podcasts(
    handle: *mut PodcastHandle,
    intent_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || intent_json.is_null() {
        return err_envelope("null argument").into_raw();
    }
    ffi_guard(
        "nmp_app_podcast_itunes_top_podcasts",
        || err_envelope("panic").into_raw(),
        || {
            let json_str = match unsafe { CStr::from_ptr(intent_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return err_envelope("invalid UTF-8").into_raw(),
            };
            let intent: TopPodcastsIntent = match serde_json::from_str(json_str) {
                Ok(intent) => intent,
                Err(e) => return err_envelope(&format!("JSON parse: {e}")).into_raw(),
            };
            let top_url = itunes::top_podcasts_url(intent.limit, &intent.storefront);
            let top_body = match fetch_body(handle, top_url, "itunes-top-podcasts") {
                Ok(body) => body,
                Err(error) => return err_envelope(&error).into_raw(),
            };
            let ranked_ids = itunes::parse_top_podcast_ids(&top_body);
            let Some(lookup_url) = itunes::lookup_ids_url(&ranked_ids) else {
                return json_envelope(&serde_json::json!({"result": []})).into_raw();
            };
            match fetch_body(handle, lookup_url, "itunes-top-lookup") {
                Ok(body) => {
                    let hits = itunes::parse_itunes_directory_results(
                        &body,
                        ItunesSearchKind::Podcast,
                    );
                    let ordered = itunes::order_hits_by_rank(hits, &ranked_ids);
                    json_envelope(&serde_json::json!({"result": ordered})).into_raw()
                }
                Err(error) => err_envelope(&error).into_raw(),
            }
        },
    )
}

fn fetch_body(handle: *mut PodcastHandle, url: String, correlation_id: &str) -> Result<String, String> {
    let handle_ref = unsafe { &*handle };
    let handler = PodcastHostOpHandler::new(handle_ref.app, Arc::clone(&handle_ref.state));
    let req = HttpRequest::get(url, [("Accept", "application/json")]);
    match handler.dispatch_http(&req, correlation_id)? {
        HttpResult::Ok { body, .. } => Ok(body),
        HttpResult::Error { message } => Err(message),
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
