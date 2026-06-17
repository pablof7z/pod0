//! Rust-owned Home category-card projections.
//!
//! The Swift category model still carries the legacy category ids and display
//! copy. Rust owns the display row facts derived from the library: which scoped
//! podcast ids are valid/subscribed and how many visible unplayed episodes they
//! contain.

use std::collections::HashSet;
use std::ffi::{c_char, CStr, CString};

use podcast_core::TriageDecision;
use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[derive(Debug, Deserialize)]
struct CategoryCardsRequest {
    #[serde(default)]
    categories: Vec<CategoryScope>,
}

#[derive(Debug, Deserialize)]
struct CategoryScope {
    category_id: String,
    #[serde(default)]
    podcast_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CategoryCardsResponse {
    categories: Vec<CategoryCardRow>,
}

#[derive(Debug, Serialize)]
struct CategoryCardRow {
    category_id: String,
    podcast_ids: Vec<String>,
    unplayed_total: usize,
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
pub extern "C" fn nmp_app_podcast_home_category_cards(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_home_category_cards", std::ptr::null_mut, || {
        let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        let request: CategoryCardsRequest = match serde_json::from_str(request_str) {
            Ok(r) => r,
            Err(_) => return std::ptr::null_mut(),
        };
        let handle_ref = unsafe { &*handle };
        let response = match handle_ref.state.library.store.lock() {
            Ok(store) => {
                let library = store.all_podcasts();
                let rows = request
                    .categories
                    .into_iter()
                    .map(|scope| {
                        let requested_ids: Vec<String> = scope
                            .podcast_ids
                            .into_iter()
                            .map(|id| id.to_lowercase())
                            .collect();
                        let requested_set: HashSet<&str> =
                            requested_ids.iter().map(String::as_str).collect();
                        let mut resolved_ids = HashSet::new();
                        let mut unplayed_total = 0;

                        for (podcast, episodes) in &library {
                            let podcast_id = podcast.id.0.to_string();
                            if !requested_set.contains(podcast_id.as_str())
                                || !store.is_subscribed(podcast.id)
                            {
                                continue;
                            }
                            resolved_ids.insert(podcast_id);
                            unplayed_total += episodes
                                .iter()
                                .filter(|episode| {
                                    let episode_id = episode.id.0.to_string();
                                    let stored_triage =
                                        store.triage_for(&episode_id).map(|(d, _, _)| d);
                                    !episode.played
                                        && episode.triage_decision.as_ref()
                                            != Some(&TriageDecision::Archived)
                                        && stored_triage != Some(&TriageDecision::Archived)
                                })
                                .count();
                        }

                        CategoryCardRow {
                            category_id: scope.category_id,
                            podcast_ids: requested_ids
                                .into_iter()
                                .filter(|id| resolved_ids.contains(id))
                                .collect(),
                            unplayed_total,
                        }
                    })
                    .collect();
                CategoryCardsResponse { categories: rows }
            }
            Err(_) => CategoryCardsResponse {
                categories: Vec::new(),
            },
        };
        encode(&response)
    })
}
