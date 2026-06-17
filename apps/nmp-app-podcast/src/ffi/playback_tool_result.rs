//! Rust-owned agent playback tool result envelopes.
//!
//! Swift dispatches playback actions and executes native audio capabilities;
//! Rust owns the semantic metadata the agent sees for library playback and
//! current now-playing state.

use std::ffi::{c_char, CStr, CString};

use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[derive(Debug, Deserialize)]
struct PlayToolResultRequest {
    episode_id: String,
    queue_position: String,
    started_playing: bool,
}

#[derive(Debug, Serialize)]
struct PlayToolResultResponse {
    ok: bool,
    episode_id: String,
    queue_position: String,
    started_playing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    episode_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    podcast_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Debug, Serialize)]
struct NowPlayingToolResultResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    episode_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    episode_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    podcast_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    podcast_title: Option<String>,
    position_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_seconds: Option<f64>,
    is_playing: bool,
    rate: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

struct EpisodeMetadata {
    episode_title: String,
    podcast_id: String,
    podcast_title: String,
    duration_seconds: Option<i64>,
}

fn episode_metadata(
    store: &crate::store::PodcastStore,
    episode_id: &str,
) -> Option<EpisodeMetadata> {
    store.all_podcasts().into_iter().find_map(|(podcast, episodes)| {
        episodes.into_iter().find(|ep| ep.id.0.to_string() == episode_id).map(|ep| {
            EpisodeMetadata {
                episode_title: ep.title,
                podcast_id: podcast.id.0.to_string(),
                podcast_title: podcast.title,
                duration_seconds: ep.duration_secs.map(|d| d.round() as i64),
            }
        })
    })
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
pub extern "C" fn nmp_app_podcast_playback_tool_result(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_playback_tool_result", std::ptr::null_mut, || {
        let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        let request: PlayToolResultRequest = match serde_json::from_str(request_str) {
            Ok(r) => r,
            Err(_) => return std::ptr::null_mut(),
        };
        let episode_id = request.episode_id.to_lowercase();
        let handle_ref = unsafe { &*handle };
        let response = match handle_ref.state.library.store.lock() {
            Ok(s) => {
                let metadata = episode_metadata(&s, &episode_id);
                PlayToolResultResponse {
                    ok: metadata.is_some(),
                    episode_id,
                    queue_position: request.queue_position,
                    started_playing: request.started_playing,
                    episode_title: metadata.as_ref().map(|m| m.episode_title.clone()),
                    podcast_title: metadata.as_ref().map(|m| m.podcast_title.clone()),
                    duration_seconds: metadata.as_ref().and_then(|m| m.duration_seconds),
                    message: metadata
                        .is_none()
                        .then(|| format!("episode not found: {}", request.episode_id)),
                }
            }
            Err(_) => PlayToolResultResponse {
                ok: false,
                episode_id,
                queue_position: request.queue_position,
                started_playing: request.started_playing,
                episode_title: None,
                podcast_title: None,
                duration_seconds: None,
                message: Some("store poisoned".to_owned()),
            },
        };
        encode(&response)
    })
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_now_playing_tool_result(
    handle: *mut PodcastHandle,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_now_playing_tool_result",
        std::ptr::null_mut,
        || {
            let handle_ref = unsafe { &*handle };
            let player = match handle_ref.state.playback.player.lock() {
                Ok(player) => player.state().clone(),
                Err(_) => {
                    return encode(&NowPlayingToolResultResponse {
                        ok: false,
                        episode_id: None,
                        episode_title: None,
                        podcast_id: None,
                        podcast_title: None,
                        position_seconds: 0.0,
                        duration_seconds: None,
                        is_playing: false,
                        rate: 1.0,
                        message: Some("player poisoned".to_owned()),
                    })
                }
            };
            let episode_id = player.episode_id.clone();
            let metadata = match (&episode_id, handle_ref.state.library.store.lock()) {
                (Some(id), Ok(s)) => episode_metadata(&s, id),
                _ => None,
            };
            let duration_seconds = if player.duration_secs > 0.0 {
                Some(player.duration_secs)
            } else {
                metadata
                    .as_ref()
                    .and_then(|m| m.duration_seconds.map(|d| d as f64))
            };
            let response = NowPlayingToolResultResponse {
                ok: true,
                episode_title: metadata
                    .as_ref()
                    .map(|m| m.episode_title.clone())
                    .or_else(|| episode_id.clone()),
                podcast_id: player
                    .podcast_id
                    .clone()
                    .or_else(|| metadata.as_ref().map(|m| m.podcast_id.clone())),
                podcast_title: metadata.as_ref().map(|m| m.podcast_title.clone()),
                episode_id,
                position_seconds: player.position_secs.max(0.0),
                duration_seconds,
                is_playing: player.is_playing,
                rate: f64::from(player.speed),
                message: None,
            };
            encode(&response)
        },
    )
}
