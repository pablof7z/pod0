//! Rust-owned CarPlay projections.
//!
//! Swift owns CarPlay template construction and native artwork loading. Rust
//! owns which episodes belong in product sections so CarPlay does not recreate
//! feed scope, played-state, triage visibility, ordering, or caps.

use std::ffi::{c_char, CStr, CString};

use podcast_core::{DownloadState, TriageDecision};
use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[derive(Debug, Deserialize)]
struct ListenNowRequest {
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Debug, Serialize)]
struct ListenNowResponse {
    in_progress_episode_ids: Vec<String>,
    latest_episode_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ShowsRequest {
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Debug, Serialize)]
struct ShowsResponse {
    shows: Vec<ShowRow>,
}

#[derive(Debug, Serialize)]
struct ShowRow {
    podcast_id: String,
    unplayed_count: usize,
}

#[derive(Debug, Deserialize)]
struct ShowEpisodesRequest {
    podcast_id: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Debug, Serialize)]
struct ShowEpisodesResponse {
    episode_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DownloadsRequest {
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Debug, Serialize)]
struct DownloadsResponse {
    episode_ids: Vec<String>,
}

#[derive(Debug)]
struct EpisodeCandidate {
    id: String,
    published_at: i64,
    position_secs: f64,
    played: bool,
}

fn default_limit() -> usize {
    30
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

fn is_archived(store: &crate::store::PodcastStore, episode: &podcast_core::Episode) -> bool {
    let episode_id = episode.id.0.to_string();
    let stored_triage = store.triage_for(&episode_id).map(|(d, _, _)| d);
    episode.triage_decision.as_ref() == Some(&TriageDecision::Archived)
        || stored_triage == Some(&TriageDecision::Archived)
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_carplay_listen_now(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_carplay_listen_now", std::ptr::null_mut, || {
        let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        let request: ListenNowRequest = match serde_json::from_str(request_str) {
            Ok(r) => r,
            Err(_) => return std::ptr::null_mut(),
        };
        let limit = request.limit.clamp(1, 200);
        let handle_ref = unsafe { &*handle };
        let response = match handle_ref.state.library.store.lock() {
            Ok(store) => {
                let mut candidates = Vec::new();
                for (podcast, episodes) in store.all_podcasts() {
                    if !store.is_subscribed(podcast.id) {
                        continue;
                    }
                    for episode in episodes {
                        let episode_id = episode.id.0.to_string();
                        if is_archived(&store, &episode) {
                            continue;
                        }
                        candidates.push(EpisodeCandidate {
                            id: episode_id,
                            published_at: episode.pub_date.timestamp(),
                            position_secs: episode.position_secs,
                            played: episode.played,
                        });
                    }
                }

                let mut in_progress: Vec<&EpisodeCandidate> = candidates
                    .iter()
                    .filter(|ep| !ep.played && ep.position_secs > 0.0)
                    .collect();
                in_progress.sort_by(|a, b| {
                    b.published_at
                        .cmp(&a.published_at)
                        .then_with(|| a.id.cmp(&b.id))
                });

                let mut latest: Vec<&EpisodeCandidate> =
                    candidates.iter().filter(|ep| !ep.played).collect();
                latest.sort_by(|a, b| {
                    b.published_at
                        .cmp(&a.published_at)
                        .then_with(|| a.id.cmp(&b.id))
                });

                ListenNowResponse {
                    in_progress_episode_ids: in_progress
                        .into_iter()
                        .take(limit)
                        .map(|ep| ep.id.clone())
                        .collect(),
                    latest_episode_ids: latest
                        .into_iter()
                        .take(limit)
                        .map(|ep| ep.id.clone())
                        .collect(),
                }
            }
            Err(_) => ListenNowResponse {
                in_progress_episode_ids: Vec::new(),
                latest_episode_ids: Vec::new(),
            },
        };
        encode(&response)
    })
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_carplay_shows(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_carplay_shows", std::ptr::null_mut, || {
        let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        let request: ShowsRequest = match serde_json::from_str(request_str) {
            Ok(r) => r,
            Err(_) => return std::ptr::null_mut(),
        };
        let limit = request.limit.clamp(1, 200);
        let handle_ref = unsafe { &*handle };
        let response = match handle_ref.state.library.store.lock() {
            Ok(store) => {
                let mut rows: Vec<(String, ShowRow)> = store
                    .all_podcasts()
                    .into_iter()
                    .filter(|(podcast, _)| store.is_subscribed(podcast.id))
                    .map(|(podcast, episodes)| {
                        let unplayed_count = episodes
                            .iter()
                            .filter(|episode| !episode.played && !is_archived(&store, episode))
                            .count();
                        (
                            podcast.title.to_lowercase(),
                            ShowRow {
                                podcast_id: podcast.id.0.to_string(),
                                unplayed_count,
                            },
                        )
                    })
                    .collect();
                rows.sort_by(|a, b| {
                    a.0.cmp(&b.0)
                        .then_with(|| a.1.podcast_id.cmp(&b.1.podcast_id))
                });
                ShowsResponse {
                    shows: rows.into_iter().take(limit).map(|(_, row)| row).collect(),
                }
            }
            Err(_) => ShowsResponse { shows: Vec::new() },
        };
        encode(&response)
    })
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_carplay_show_episodes(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_carplay_show_episodes", std::ptr::null_mut, || {
        let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        let request: ShowEpisodesRequest = match serde_json::from_str(request_str) {
            Ok(r) => r,
            Err(_) => return std::ptr::null_mut(),
        };
        let podcast_id = request.podcast_id.to_lowercase();
        let limit = request.limit.clamp(1, 200);
        let handle_ref = unsafe { &*handle };
        let response = match handle_ref.state.library.store.lock() {
            Ok(store) => {
                let mut episode_rows = Vec::new();
                for (podcast, episodes) in store.all_podcasts() {
                    if podcast.id.0.to_string() != podcast_id || !store.is_subscribed(podcast.id) {
                        continue;
                    }
                    episode_rows = episodes
                        .into_iter()
                        .filter(|episode| !is_archived(&store, episode))
                        .map(|episode| (episode.pub_date.timestamp(), episode.id.0.to_string()))
                        .collect();
                    break;
                }
                episode_rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                ShowEpisodesResponse {
                    episode_ids: episode_rows
                        .into_iter()
                        .take(limit)
                        .map(|(_, id)| id)
                        .collect(),
                }
            }
            Err(_) => ShowEpisodesResponse {
                episode_ids: Vec::new(),
            },
        };
        encode(&response)
    })
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_carplay_downloads(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_carplay_downloads", std::ptr::null_mut, || {
        let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        let request: DownloadsRequest = match serde_json::from_str(request_str) {
            Ok(r) => r,
            Err(_) => return std::ptr::null_mut(),
        };
        let limit = request.limit.clamp(1, 200);
        let handle_ref = unsafe { &*handle };
        let response = match handle_ref.state.library.store.lock() {
            Ok(store) => {
                let mut rows: Vec<(i64, String)> = store
                    .all_podcasts()
                    .into_iter()
                    .flat_map(|(_, episodes)| episodes)
                    .filter(|episode| !is_archived(&store, episode))
                    .filter(|episode| matches!(episode.download_state, DownloadState::Downloaded { .. }))
                    .map(|episode| (episode.pub_date.timestamp(), episode.id.0.to_string()))
                    .collect();
                rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                DownloadsResponse {
                    episode_ids: rows.into_iter().take(limit).map(|(_, id)| id).collect(),
                }
            }
            Err(_) => DownloadsResponse {
                episode_ids: Vec::new(),
            },
        };
        encode(&response)
    })
}
