//! Rust-owned agent empty-state context.
//!
//! Swift owns the displayed copy. Rust owns which capability context applies to
//! the user's current library/playback state.

use std::ffi::{c_char, CString};

use podcast_core::TriageDecision;
use serde::Serialize;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[derive(Debug, Serialize)]
struct EmptyStateResponse {
    suggestion_context: &'static str,
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
pub extern "C" fn nmp_app_podcast_agent_empty_state(
    handle: *mut PodcastHandle,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_agent_empty_state", std::ptr::null_mut, || {
        let handle_ref = unsafe { &*handle };
        let response = match handle_ref.state.library.store.lock() {
            Ok(store) => {
                let library = store.all_podcasts();
                let has_in_progress = library.iter().any(|(_, episodes)| {
                    episodes.iter().any(|episode| {
                        !episode.played && episode.position_secs > 0.0 && !is_archived(&store, episode)
                    })
                });
                let has_subscriptions = library
                    .iter()
                    .any(|(podcast, _)| store.is_subscribed(podcast.id));
                EmptyStateResponse {
                    suggestion_context: if has_in_progress {
                        "resume"
                    } else if has_subscriptions {
                        "subscribed"
                    } else {
                        "onboarding"
                    },
                }
            }
            Err(_) => EmptyStateResponse {
                suggestion_context: "onboarding",
            },
        };
        encode(&response)
    })
}
