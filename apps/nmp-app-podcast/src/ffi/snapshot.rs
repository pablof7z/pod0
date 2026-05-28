//! Snapshot entry points the host calls against a [`PodcastHandle`].
//!
//! [`PodcastUpdate`] type definition lives in `snapshot_update.rs`;
//! per-projection types live in [`super::projections`]; queue, owned-podcast,
//! and category build helpers live in `snapshot_queue/owned/categories` siblings.

pub use super::snapshot_update::PodcastUpdate;

use std::ffi::{c_char, CString};
use std::sync::atomic::Ordering;

use super::handle::PodcastHandle;
use super::helpers::strip_html;
use super::projections::{
    AccountSummary, AgentSnapshot, ChapterSummary, EpisodeSummary, PodcastSummary,
    SettingsSnapshot, VoiceState,
};
use super::snapshot_categories::build_category_aggregate;
use super::snapshot_downloads::build_downloads_snapshot;
use super::snapshot_owned::collect_owned_podcasts;
use super::snapshot_queue::resolve_queue_rows;
use crate::inbox_handler::build_inbox;

/// Build the JSON payload for one snapshot tick.
///
/// Reads `player_actor`, `store`, and `rev` from `handle` under their
/// respective short-duration locks, assembles the typed [`PodcastUpdate`],
/// and serializes it. Failures degrade to the byte-compatible legacy stub
/// (D6).
/// Build the typed [`PodcastUpdate`] directly from the handle state.
///
/// Rust-native path — no JSON round-trip. Used by the TUI and other
/// Rust consumers that want typed projections without paying serde
/// serialization + deserialization.
pub fn build_podcast_update(handle: &PodcastHandle) -> PodcastUpdate {
    let rev = handle.rev.load(Ordering::Relaxed);

    let now_playing = handle.player_actor.lock().ok().and_then(|a| {
        let s = a.state().clone();
        if s.episode_id.is_some() { Some(s) } else { None }
    });

    // Snapshot caches before the store lock so we don't hold two locks at once.
    let transcripts = handle.transcripts.lock().ok().map(|t| t.clone()).unwrap_or_default();
    let categories_cache: std::collections::HashMap<String, Vec<String>> =
        handle.categories.lock().ok().map(|c| c.clone()).unwrap_or_default();

    // Single store lock → library + memory_facts + settings.
    let (library, memory_facts, settings) = handle.store.lock().ok().map(|s| {
        let library: Vec<PodcastSummary> = s
            .all_podcasts()
            .into_iter()
            .map(|(podcast, episodes)| PodcastSummary {
                id: podcast.id.0.to_string(),
                title: podcast.title.clone(),
                episode_count: episodes.len(),
                unplayed_count: episodes.iter().filter(|e| !e.played).count(),
                artwork_url: podcast.image_url.as_ref().map(|u| u.to_string()),
                feed_url: podcast.feed_url.as_ref().map(|u| u.to_string()),
                author: if podcast.author.is_empty() {
                    None
                } else {
                    Some(podcast.author.clone())
                },
                description: Some(strip_html(&podcast.description))
                    .filter(|d| !d.is_empty()),
                auto_download: s.is_auto_download_enabled(podcast.id),
                episodes: episodes
                    .iter()
                    .map(|ep| {
                        let ep_id = ep.id.0.to_string();
                        let transcript = s.transcript_for(&ep_id).map(str::to_owned);
                        let transcript_entries =
                            transcripts.get(&ep_id).cloned().unwrap_or_default();
                        let ai_categories =
                            categories_cache.get(&ep_id).cloned().unwrap_or_default();
                        let ad_segments = s.ad_segments_for(&ep_id).to_vec();
                        EpisodeSummary {
                            id: ep_id.clone(),
                            title: ep.title.clone(),
                            podcast_id: Some(podcast.id.0.to_string()),
                            podcast_title: Some(podcast.title.clone()),
                            duration_secs: ep.duration_secs,
                            artwork_url: ep.image_url.as_ref().map(|u| u.to_string()),
                            published_at: Some(ep.pub_date.timestamp()),
                            download_path: s.local_path_for(&ep.id).map(str::to_owned),
                            description: Some(strip_html(&ep.description))
                                .filter(|d| !d.is_empty()),
                            transcript,
                            transcript_url: ep
                                .publisher_transcript_url
                                .as_ref()
                                .map(|u| u.to_string()),
                            transcript_entries,
                            chapters: ep
                                .chapters
                                .as_ref()
                                .map(|cs| {
                                    cs.iter()
                                        .map(|c| ChapterSummary {
                                            start_secs: c.start_secs,
                                            end_secs: c.end_secs,
                                            title: c.title.clone(),
                                            image_url: c
                                                .image_url
                                                .as_ref()
                                                .map(|u| u.to_string()),
                                            url: c.link_url.as_ref().map(|u| u.to_string()),
                                            is_ai_generated: c.is_ai_generated,
                                        })
                                        .collect()
                                })
                                .unwrap_or_default(),
                            playback_position_secs: s.position_for(&ep_id),
                            ai_categories,
                            ad_segments,
                            played: ep.played,
                            starred: ep.is_starred,
                        }
                    })
                    .collect(),
            })
            .collect();
        let settings = SettingsSnapshot {
            has_completed_onboarding: s.has_completed_onboarding(),
            auto_skip_ads_enabled: s.auto_skip_ads_enabled(),
            auto_play_next: s.auto_play_next(),
            auto_mark_played_at_end: s.auto_mark_played_at_end(),
            headphone_double_tap_action: s.headphone_double_tap_action().to_owned(),
            headphone_triple_tap_action: s.headphone_triple_tap_action().to_owned(),
            skip_forward_secs: s.skip_forward_secs(),
            skip_backward_secs: s.skip_backward_secs(),
        };
        (library, s.all_memory_facts(), settings)
    })
    .unwrap_or_default();

    let categories = build_category_aggregate(&library);
    let search_results = handle.search_results.lock().ok().map(|r| r.clone()).unwrap_or_default();
    let nostr_results = handle.nostr_results.lock().ok().map(|r| r.clone()).unwrap_or_default();
    let briefing = handle.briefing.lock().ok().and_then(|b| b.clone());
    let queue_ids: Vec<String> = handle.queue.lock().ok()
        .map(|q| q.items().to_vec()).unwrap_or_default();
    let queue = resolve_queue_rows(&queue_ids, &library);
    let wiki_articles = handle.wiki_articles.lock().ok().map(|w| w.clone()).unwrap_or_default();
    let wiki_search_results = handle.wiki_search_results.lock().ok().map(|w| w.clone()).unwrap_or_default();
    let picks = handle.picks.lock().ok().map(|p| p.clone()).unwrap_or_default();
    let agent_tasks = handle.agent_tasks.lock().ok().map(|t| t.clone()).unwrap_or_default();
    let knowledge_search_results = handle.knowledge_search_results.lock().ok()
        .map(|r| r.clone()).unwrap_or_default();
    let tts_episodes = handle.tts_episodes.lock().ok().map(|r| r.clone()).unwrap_or_default();
    let clips = crate::clip_handler::project_clips(&handle.clips, &library);
    let inbox = build_inbox(&handle.store, &handle.dismissed_episode_ids, &handle.inbox_triage_cache);
    let owned_podcasts = collect_owned_podcasts(handle);
    let downloads = handle.download_queue.lock().ok()
        .and_then(|q| build_downloads_snapshot(&q));

    // Project comments for the now-playing episode from the cache.
    let comments = handle
        .comments_cache
        .lock()
        .ok()
        .and_then(|cache| {
            now_playing
                .as_ref()
                .and_then(|np| np.episode_id.as_deref())
                .and_then(|ep_id| cache.get(ep_id).cloned())
        })
        .unwrap_or_default();

    let active_account = handle.identity.lock().ok().and_then(|id| {
        id.npub.as_ref().map(|npub| AccountSummary {
            npub: npub.clone(),
            mode: "local_key".into(),
            display_name: id.display_name.clone(),
            picture_url: id.picture_url.clone(),
        })
    });

    let social = handle.social.lock().ok().and_then(|s| s.clone());

    let voice = handle.voice_state.lock().ok().and_then(|v| {
        let snap = v.clone();
        (snap != VoiceState::default()).then_some(snap)
    });

    let agent = handle.conversation.lock().ok().and_then(|c| {
        if c.is_empty() && !handle.agent_touched.load(Ordering::Relaxed) {
            None
        } else {
            Some(AgentSnapshot {
                messages: c.clone(),
                is_busy: handle.agent_busy.load(Ordering::Relaxed),
            })
        }
    });

    PodcastUpdate {
        rev,
        now_playing,
        library,
        active_account,
        search_results,
        nostr_results,
        settings,
        comments,
        queue,
        wiki_articles,
        wiki_search_results,
        picks,
        agent_tasks,
        knowledge_search_results,
        memory_facts,
        tts_episodes,
        clips,
        inbox,
        owned_podcasts,
        downloads,
        voice,
        agent,
        categories,
        briefing,
        social,
        ..PodcastUpdate::default()
    }
}

fn build_snapshot_payload(handle: &PodcastHandle) -> String {
    let rev = handle.rev.load(Ordering::Relaxed);

    // Fast path: skip re-serialization when rev hasn't changed.
    if let Ok(cache) = handle.snapshot_cache.lock() {
        if let Some((cached_rev, ref cached_json)) = *cache {
            if cached_rev == rev {
                return cached_json.clone();
            }
        }
    }

    let update = build_podcast_update(handle);
    let json = serde_json::to_string(&update)
        .unwrap_or_else(|_| r#"{"running":true,"rev":0,"schema_version":1}"#.to_owned());

    if let Ok(mut cache) = handle.snapshot_cache.lock() {
        *cache = Some((rev, json.clone()));
    }
    json
}

impl PodcastHandle {
    /// Build the typed [`PodcastUpdate`] directly from the handle state.
    ///
    /// Rust-native path — no JSON round-trip. Used by the TUI and other
    /// Rust consumers that want typed projections without paying serde
    /// serialization + deserialization.
    pub fn update(&self) -> PodcastUpdate {
        build_podcast_update(self)
    }
}

/// Serialize the current app state into a JSON C string.
///
/// Returns null on any failure (null handle, `CString` nul-byte conflict).
/// The returned pointer is owned by the caller; pass it to
/// [`nmp_app_podcast_snapshot_free`] when done. Payload shape is
/// defined by [`PodcastUpdate`].
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_snapshot(handle: *mut PodcastHandle) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller guarantees `handle` is a valid pointer returned by
    // `nmp_app_podcast_register` and not yet freed.
    let handle = unsafe { &*handle };

    let payload = build_snapshot_payload(handle);
    let Ok(cstr) = CString::new(payload) else {
        return std::ptr::null_mut();
    };
    cstr.into_raw()
}

/// Cheap rev probe: reads the atomic counter without serializing the payload.
/// Returns `0` on null handle. Use before `nmp_app_podcast_snapshot` to skip
/// the full JSON round-trip when nothing has changed.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_snapshot_rev(handle: *mut PodcastHandle) -> u64 {
    if handle.is_null() { return 0; }
    let handle = unsafe { &*handle };
    handle.rev.load(std::sync::atomic::Ordering::Relaxed)
}

/// Free a snapshot string previously returned by [`nmp_app_podcast_snapshot`].
/// Null pointer is a silent no-op.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_snapshot_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: caller guarantees `ptr` came from `CString::into_raw` in
    // `nmp_app_podcast_snapshot` and has not been freed.
    unsafe {
        let _ = CString::from_raw(ptr);
    }
}

/// Drop the handle and free associated resources.
/// Idempotent: null pointer is a silent no-op. The handle MUST NOT be used
/// after this call.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_unregister(handle: *mut PodcastHandle) {
    if handle.is_null() {
        return;
    }
    // SAFETY: caller guarantees `handle` came from `nmp_app_podcast_register`
    // and has not already been freed.
    let boxed = unsafe { Box::from_raw(handle) };
    let _ = boxed.app;
}

// Tests split into snapshot_tests.rs + snapshot_tests_ext.rs; #[path] keeps private items in scope.
#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "snapshot_tests_ext.rs"]
mod tests_ext;
