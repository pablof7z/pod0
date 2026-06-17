//! Rust-owned agent episode-mutation result envelope.
//!
//! Swift dispatches the mutation action, but Rust owns the authoritative
//! episode/podcast metadata used in the agent tool result.

use std::ffi::{c_char, CStr, CString};

use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[derive(Debug, Deserialize)]
struct EpisodeMutationToolResultRequest {
    episode_id: String,
    state: String,
}

#[derive(Debug, Serialize)]
struct EpisodeMutationToolResultResponse {
    ok: bool,
    episode_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    podcast_id: Option<String>,
    episode_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    podcast_title: Option<String>,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl EpisodeMutationToolResultResponse {
    fn error(episode_id: String, state: String, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            episode_id,
            podcast_id: None,
            episode_title: String::new(),
            podcast_title: None,
            state,
            message: Some(message.into()),
        }
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_episode_mutation_tool_result(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_episode_mutation_tool_result",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: EpisodeMutationToolResultRequest =
                match serde_json::from_str(request_str) {
                    Ok(r) => r,
                    Err(_) => return std::ptr::null_mut(),
                };
            let episode_id = request.episode_id.to_lowercase();
            let handle_ref = unsafe { &*handle };
            let response = match handle_ref.state.library.store.lock() {
                Ok(s) => s
                    .all_podcasts()
                    .into_iter()
                    .find_map(|(podcast, episodes)| {
                        episodes
                            .into_iter()
                            .find(|ep| ep.id.0.to_string() == episode_id)
                            .map(|episode| EpisodeMutationToolResultResponse {
                                ok: true,
                                episode_id: episode_id.clone(),
                                podcast_id: Some(podcast.id.0.to_string()),
                                episode_title: episode.title,
                                podcast_title: Some(podcast.title),
                                state: request.state.clone(),
                                message: None,
                            })
                    })
                    .unwrap_or_else(|| {
                        EpisodeMutationToolResultResponse::error(
                            episode_id,
                            request.state,
                            format!("episode not found: {}", request.episode_id),
                        )
                    }),
                Err(_) => EpisodeMutationToolResultResponse::error(
                    episode_id,
                    request.state,
                    "store poisoned",
                ),
            };
            match serde_json::to_string(&response) {
                Ok(json) => match CString::new(json) {
                    Ok(c) => c.into_raw(),
                    Err(_) => std::ptr::null_mut(),
                },
                Err(_) => std::ptr::null_mut(),
            }
        },
    )
}
