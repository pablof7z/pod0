//! Rust-owned Home screen projections.
//!
//! Swift renders the native Home UI, but product rules such as "continue
//! listening means unplayed, non-archived, in-progress episodes from the last
//! two weeks" belong in the kernel.

use std::collections::HashSet;
use std::ffi::{c_char, CStr, CString};

use chrono::Utc;
use podcast_core::TriageDecision;
use serde::{Deserialize, Serialize};

use crate::ffi::guard::ffi_guard;
use crate::ffi::handle::PodcastHandle;

const CONTINUE_LISTENING_WINDOW_SECS: i64 = 14 * 24 * 60 * 60;

#[derive(Debug, Deserialize)]
struct HomeContinueListeningRequest {
    #[serde(default)]
    podcast_ids: Vec<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct HomeContinueListeningResponse {
    episode_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct HomeTriageRollupRequest {
    #[serde(default)]
    podcast_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct HomeTriageRollupResponse {
    inbox: usize,
    archived: usize,
    shows: usize,
}

#[derive(Debug, Deserialize)]
struct HomeSubscriptionListRequest {
    #[serde(default)]
    podcast_ids: Vec<String>,
    #[serde(default)]
    filter: Option<String>,
}

#[derive(Debug, Serialize)]
struct HomeSubscriptionListResponse {
    podcast_ids: Vec<String>,
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_home_continue_listening(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_home_continue_listening",
        std::ptr::null_mut,
        || {
            let request = if request_json.is_null() {
                HomeContinueListeningRequest {
                    podcast_ids: Vec::new(),
                    limit: None,
                }
            } else {
                let raw = unsafe { CStr::from_ptr(request_json) }
                    .to_string_lossy()
                    .into_owned();
                serde_json::from_str::<HomeContinueListeningRequest>(&raw).unwrap_or(
                    HomeContinueListeningRequest {
                        podcast_ids: Vec::new(),
                        limit: None,
                    },
                )
            };
            let allowed: HashSet<String> = request
                .podcast_ids
                .into_iter()
                .map(|id| id.to_ascii_lowercase())
                .collect();
            let limit = request.limit.unwrap_or(20).max(1).min(100);
            let cutoff = Utc::now().timestamp() - CONTINUE_LISTENING_WINDOW_SECS;
            let handle_ref = unsafe { &*handle };
            let mut rows: Vec<(i64, String)> = match handle_ref.state.library.store.lock() {
                Ok(store) => store
                    .all_podcasts()
                    .into_iter()
                    .flat_map(|(_podcast, episodes)| episodes.iter())
                    .filter(|episode| {
                        let podcast_id = episode.podcast_id.0.to_string().to_ascii_lowercase();
                        (allowed.is_empty() || allowed.contains(&podcast_id))
                            && !episode.played
                            && episode.position_secs > 0.0
                            && episode.triage_decision != Some(TriageDecision::Archived)
                            && episode.pub_date.timestamp() >= cutoff
                    })
                    .map(|episode| (episode.pub_date.timestamp(), episode.id.0.to_string()))
                    .collect(),
                Err(_) => return std::ptr::null_mut(),
            };
            rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            rows.truncate(limit);
            let response = HomeContinueListeningResponse {
                episode_ids: rows.into_iter().map(|(_, id)| id).collect(),
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

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_home_triage_rollup(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_home_triage_rollup",
        std::ptr::null_mut,
        || {
            let request = if request_json.is_null() {
                HomeTriageRollupRequest {
                    podcast_ids: Vec::new(),
                }
            } else {
                let raw = unsafe { CStr::from_ptr(request_json) }
                    .to_string_lossy()
                    .into_owned();
                serde_json::from_str::<HomeTriageRollupRequest>(&raw).unwrap_or(
                    HomeTriageRollupRequest {
                        podcast_ids: Vec::new(),
                    },
                )
            };
            let allowed: HashSet<String> = request
                .podcast_ids
                .into_iter()
                .map(|id| id.to_ascii_lowercase())
                .collect();
            let handle_ref = unsafe { &*handle };
            let mut inbox = 0usize;
            let mut archived = 0usize;
            let mut shows = HashSet::new();
            match handle_ref.state.library.store.lock() {
                Ok(store) => {
                    for (_podcast, episodes) in store.all_podcasts() {
                        for episode in episodes.iter() {
                            let podcast_id = episode.podcast_id.0.to_string().to_ascii_lowercase();
                            if !allowed.is_empty() && !allowed.contains(&podcast_id) {
                                continue;
                            }
                            match episode.triage_decision.as_ref() {
                                Some(&TriageDecision::Inbox) => {
                                    shows.insert(podcast_id);
                                    if !episode.played {
                                        inbox += 1;
                                    }
                                }
                                Some(&TriageDecision::Archived) => {
                                    shows.insert(podcast_id);
                                    archived += 1;
                                }
                                None => {}
                            }
                        }
                    }
                }
                Err(_) => return std::ptr::null_mut(),
            }
            let response = HomeTriageRollupResponse {
                inbox,
                archived,
                shows: shows.len(),
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

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_home_subscription_list(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_home_subscription_list",
        std::ptr::null_mut,
        || {
            let request = if request_json.is_null() {
                HomeSubscriptionListRequest {
                    podcast_ids: Vec::new(),
                    filter: None,
                }
            } else {
                let raw = unsafe { CStr::from_ptr(request_json) }
                    .to_string_lossy()
                    .into_owned();
                serde_json::from_str::<HomeSubscriptionListRequest>(&raw).unwrap_or(
                    HomeSubscriptionListRequest {
                        podcast_ids: Vec::new(),
                        filter: None,
                    },
                )
            };
            let allowed: HashSet<String> = request
                .podcast_ids
                .into_iter()
                .map(|id| id.to_ascii_lowercase())
                .collect();
            let filter = request.filter.unwrap_or_else(|| "all".to_owned());
            let handle_ref = unsafe { &*handle };
            let mut rows: Vec<(Option<i64>, String, String)> =
                match handle_ref.state.library.store.lock() {
                    Ok(store) => store
                        .all_podcasts()
                        .into_iter()
                        .filter(|(podcast, _)| {
                            let podcast_id = podcast.id.0.to_string().to_ascii_lowercase();
                            store.is_subscribed(podcast.id)
                                && podcast.feed_url.is_some()
                                && (allowed.is_empty() || allowed.contains(&podcast_id))
                        })
                        .filter(|(_, episodes)| match filter.as_str() {
                            "unplayed" => episodes.iter().any(|episode| {
                                !episode.played
                                    && episode.triage_decision.as_ref()
                                        != Some(&TriageDecision::Archived)
                            }),
                            "downloaded" => episodes
                                .iter()
                                .any(|episode| store.local_path_for(&episode.id).is_some()),
                            "transcribed" => episodes
                                .iter()
                                .any(|episode| {
                                    store.transcript_for(&episode.id.0.to_string()).is_some()
                                }),
                            _ => true,
                        })
                        .map(|(podcast, episodes)| {
                            let latest = episodes
                                .iter()
                                .map(|episode| episode.pub_date.timestamp())
                                .max();
                            (
                                latest,
                                podcast.title.to_ascii_lowercase(),
                                podcast.id.0.to_string(),
                            )
                        })
                        .collect(),
                    Err(_) => return std::ptr::null_mut(),
                };
            rows.sort_by(|a, b| match (a.0, b.0) {
                (Some(left), Some(right)) if left != right => right.cmp(&left),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                _ => a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2)),
            });
            let response = HomeSubscriptionListResponse {
                podcast_ids: rows.into_iter().map(|(_, _, id)| id).collect(),
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
