//! Rust-owned storage projections.
//!
//! Native enumerates raw files because the downloads directory is an OS
//! capability. Rust owns the semantic join against the podcast library, orphan
//! classification, show grouping, totals, and ordering.

use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, CStr, CString};

use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[derive(Debug, Deserialize)]
struct StorageBreakdownRequest {
    #[serde(default)]
    files: Vec<StorageFileFact>,
}

#[derive(Debug, Deserialize)]
struct StorageFileFact {
    #[serde(default)]
    episode_id: Option<String>,
    #[serde(default)]
    url: String,
    #[serde(default)]
    bytes: i64,
}

#[derive(Debug, Serialize)]
struct StorageBreakdownResponse {
    total_bytes: i64,
    shows: Vec<StorageShowRow>,
    orphan_bytes: i64,
    orphan_count: usize,
    orphan_urls: Vec<String>,
}

#[derive(Debug, Serialize)]
struct StorageShowRow {
    subscription_id: String,
    title: String,
    bytes: i64,
    episode_count: usize,
    episode_ids: Vec<String>,
}

#[derive(Debug)]
struct ShowAccum {
    title: String,
    bytes: i64,
    episode_ids: HashSet<String>,
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
pub extern "C" fn nmp_app_podcast_storage_breakdown(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_storage_breakdown",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: StorageBreakdownRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return std::ptr::null_mut(),
            };
            let handle_ref = unsafe { &*handle };
            let response = match handle_ref.state.library.store.lock() {
                Ok(store) => {
                    let mut episode_to_show: HashMap<String, (String, String)> = HashMap::new();
                    for (podcast, episodes) in store.all_podcasts() {
                        let podcast_id = podcast.id.0.to_string();
                        let title = if podcast.title.is_empty() {
                            "Unknown show".to_string()
                        } else {
                            podcast.title
                        };
                        for episode in episodes {
                            episode_to_show.insert(
                                episode.id.0.to_string().to_lowercase(),
                                (podcast_id.clone(), title.clone()),
                            );
                        }
                    }

                    let mut total_bytes = 0;
                    let mut orphan_bytes = 0;
                    let mut orphan_urls = HashSet::new();
                    let mut by_show: HashMap<String, ShowAccum> = HashMap::new();

                    for file in request.files {
                        let bytes = file.bytes.max(0);
                        total_bytes += bytes;
                        let matched = file
                            .episode_id
                            .as_deref()
                            .map(|id| id.to_lowercase())
                            .and_then(|id| episode_to_show.get(&id).cloned().map(|show| (id, show)));
                        match matched {
                            Some((episode_id, (podcast_id, title))) => {
                                let entry = by_show.entry(podcast_id).or_insert_with(|| ShowAccum {
                                    title,
                                    bytes: 0,
                                    episode_ids: HashSet::new(),
                                });
                                entry.bytes += bytes;
                                entry.episode_ids.insert(episode_id);
                            }
                            None => {
                                orphan_bytes += bytes;
                                orphan_urls.insert(file.url);
                            }
                        }
                    }

                    let mut shows: Vec<StorageShowRow> = by_show
                        .into_iter()
                        .map(|(subscription_id, entry)| {
                            let mut episode_ids: Vec<String> =
                                entry.episode_ids.into_iter().collect();
                            episode_ids.sort();
                            StorageShowRow {
                                subscription_id,
                                title: entry.title,
                                bytes: entry.bytes,
                                episode_count: episode_ids.len(),
                                episode_ids,
                            }
                        })
                        .collect();
                    shows.sort_by(|a, b| {
                        b.bytes
                            .cmp(&a.bytes)
                            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                            .then_with(|| a.subscription_id.cmp(&b.subscription_id))
                    });
                    let mut orphan_urls: Vec<String> = orphan_urls.into_iter().collect();
                    orphan_urls.sort();
                    StorageBreakdownResponse {
                        total_bytes,
                        shows,
                        orphan_bytes,
                        orphan_count: orphan_urls.len(),
                        orphan_urls,
                    }
                }
                Err(_) => StorageBreakdownResponse {
                    total_bytes: 0,
                    shows: Vec::new(),
                    orphan_bytes: 0,
                    orphan_count: 0,
                    orphan_urls: Vec::new(),
                },
            };
            encode(&response)
        },
    )
}
