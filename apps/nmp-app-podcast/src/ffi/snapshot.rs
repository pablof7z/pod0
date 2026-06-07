//! Snapshot entry points the host calls against a [`PodcastHandle`].
//!
//! [`PodcastUpdate`] type definition lives in `snapshot_update.rs`;
//! per-projection types live in [`super::projections`]; queue, owned-podcast,
//! and category build helpers live in `snapshot_queue/owned/categories` siblings.

pub use super::snapshot_update::{AppRelayRow, PodcastUpdate};

use std::ffi::{c_char, CString};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::handle::PodcastHandle;
use super::projections::{
    AccountSummary, AgentSnapshot, PodcastSummary, SettingsSnapshot, VoiceState,
};
use super::snapshot_categories::build_category_aggregate;
use super::snapshot_downloads::build_downloads_snapshot;
use super::snapshot_owned::collect_owned_podcasts;
use super::snapshot_queue::resolve_queue_rows;
use crate::inbox_handler::{build_inbox, maybe_enqueue_triage_with_signal};

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
        if s.episode_id.is_some() {
            Some(s)
        } else {
            None
        }
    });

    // Snapshot caches before the store lock so we don't hold two locks at once.
    let transcripts = handle
        .transcripts
        .lock()
        .ok()
        .map(|t| t.clone())
        .unwrap_or_default();
    let categories_cache: std::collections::HashMap<String, Vec<String>> = handle
        .categories
        .lock()
        .ok()
        .map(|c| c.clone())
        .unwrap_or_default();

    // Single store lock → library + memory_facts + settings.
    let (library, memory_facts, settings) = handle
        .store
        .lock()
        .ok()
        .map(|s| {
            let library = super::snapshot_library::build_library_snapshot(
                handle,
                &s,
                &transcripts,
                &categories_cache,
            );
            let settings = SettingsSnapshot {
                has_completed_onboarding: s.has_completed_onboarding(),
                auto_skip_ads_enabled: s.auto_skip_ads_enabled(),
                auto_play_next: s.auto_play_next(),
                auto_mark_played_at_end: s.auto_mark_played_at_end(),
                headphone_double_tap_action: s.headphone_double_tap_action().to_owned(),
                headphone_triple_tap_action: s.headphone_triple_tap_action().to_owned(),
                skip_forward_secs: s.skip_forward_secs(),
                skip_backward_secs: s.skip_backward_secs(),
                default_playback_rate: s.default_playback_rate(),
                auto_delete_downloads_after_played: s.auto_delete_downloads_after_played(),
                agent_initial_model: s.agent_initial_model().to_owned(),
                agent_initial_model_name: s.agent_initial_model_name().to_owned(),
                agent_thinking_model: s.agent_thinking_model().to_owned(),
                agent_thinking_model_name: s.agent_thinking_model_name().to_owned(),
                memory_compilation_model: s.memory_compilation_model().to_owned(),
                memory_compilation_model_name: s.memory_compilation_model_name().to_owned(),
                wiki_model: s.wiki_model().to_owned(),
                wiki_model_name: s.wiki_model_name().to_owned(),
                categorization_model: s.categorization_model().to_owned(),
                categorization_model_name: s.categorization_model_name().to_owned(),
                chapter_compilation_model: s.chapter_compilation_model().to_owned(),
                chapter_compilation_model_name: s.chapter_compilation_model_name().to_owned(),
                embeddings_model: s.embeddings_model().to_owned(),
                embeddings_model_name: s.embeddings_model_name().to_owned(),
                image_generation_model: s.image_generation_model().to_owned(),
                image_generation_model_name: s.image_generation_model_name().to_owned(),
                reranker_enabled: s.reranker_enabled(),
                open_router_credential_source: s.open_router_credential_source().to_owned(),
                open_router_byok_key_id: s.open_router_byok_key_id().map(|s| s.to_owned()),
                open_router_byok_key_label: s.open_router_byok_key_label().map(|s| s.to_owned()),
                open_router_connected_at: s.open_router_connected_at(),
                ollama_credential_source: s.ollama_credential_source().to_owned(),
                ollama_byok_key_id: s.ollama_byok_key_id().map(|s| s.to_owned()),
                ollama_byok_key_label: s.ollama_byok_key_label().map(|s| s.to_owned()),
                ollama_connected_at: s.ollama_connected_at(),
                ollama_chat_url: s.ollama_chat_url().to_owned(),
                eleven_labs_credential_source: s.eleven_labs_credential_source().to_owned(),
                eleven_labs_byok_key_id: s.eleven_labs_byok_key_id().map(|s| s.to_owned()),
                eleven_labs_byok_key_label: s.eleven_labs_byok_key_label().map(|s| s.to_owned()),
                eleven_labs_connected_at: s.eleven_labs_connected_at(),
                stt_provider: s.stt_provider().to_owned(),
                effective_stt_provider: s.effective_stt_provider().to_owned(),
                effective_stt_provider_requires_key: crate::store::stt_policy::requires_key(
                    s.effective_stt_provider(),
                ),
                open_router_whisper_model: s.open_router_whisper_model().to_owned(),
                assembly_ai_stt_model: s.assembly_ai_stt_model().to_owned(),
                eleven_labs_stt_model: s.eleven_labs_stt_model().to_owned(),
                eleven_labs_tts_model: s.eleven_labs_tts_model().to_owned(),
                eleven_labs_voice_id: s.eleven_labs_voice_id().to_owned(),
                eleven_labs_voice_name: s.eleven_labs_voice_name().to_owned(),
                blossom_server_url: s.blossom_server_url().to_owned(),
                youtube_extractor_url: s.youtube_extractor_url().map(|s| s.to_owned()),
                local_model_id: s.local_model_id().map(|s| s.to_owned()),
                wiki_auto_generate_on_transcript_ingest: s
                    .wiki_auto_generate_on_transcript_ingest(),
                auto_ingest_publisher_transcripts: s.auto_ingest_publisher_transcripts(),
                auto_fallback_to_scribe: s.auto_fallback_to_scribe(),
                notify_on_new_episodes: s.notify_on_new_episodes(),
                nostr_enabled: s.nostr_enabled(),
                nostr_relay_url: s.nostr_relay_url().to_owned(),
                nostr_public_relays: s.nostr_public_relays().to_vec(),
                nostr_profile_name: s.nostr_profile_name().to_owned(),
                nostr_profile_about: s.nostr_profile_about().to_owned(),
                nostr_profile_picture: s.nostr_profile_picture().to_owned(),
                nostr_public_key_hex: s.nostr_public_key_hex().map(|s| s.to_owned()),
            };
            (library, s.all_memory_facts(), settings)
        })
        .unwrap_or_default();

    let subscribed_library: Vec<PodcastSummary> = library
        .iter()
        .filter(|p| p.is_subscribed)
        .cloned()
        .collect();
    let categories = build_category_aggregate(&subscribed_library);
    // Agent-prompt inventory context (kernel-owned selection/ordering/capping).
    // Derived from the already-assembled subscribed slice so it reuses resolved
    // position/played/triage/pub-date without a second store lock. `None` when
    // no shows are followed so a fresh install stays byte-identical to the stub.
    let agent_context = if subscribed_library.is_empty() {
        None
    } else {
        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Some(super::agent_context::build_agent_context(
            &subscribed_library,
            now_unix,
        ))
    };
    let search_results = handle
        .search_results
        .lock()
        .ok()
        .map(|r| r.clone())
        .unwrap_or_default();
    let nostr_results = handle
        .nostr_results
        .lock()
        .ok()
        .map(|r| r.clone())
        .unwrap_or_default();
    let queue_ids: Vec<String> = handle
        .queue
        .lock()
        .ok()
        .map(|q| q.items().to_vec())
        .unwrap_or_default();
    let queue = resolve_queue_rows(&queue_ids, &library);
    let wiki_articles = handle
        .wiki_articles
        .lock()
        .ok()
        .map(|w| w.clone())
        .unwrap_or_default();
    let wiki_search_results = handle
        .wiki_search_results
        .lock()
        .ok()
        .map(|w| w.clone())
        .unwrap_or_default();
    let picks = handle
        .picks
        .lock()
        .ok()
        .map(|p| p.clone())
        .unwrap_or_default();
    let agent_tasks = handle
        .agent_tasks
        .lock()
        .ok()
        .map(|t| t.clone())
        .unwrap_or_default();
    let knowledge_search_results = handle
        .knowledge_search_results
        .lock()
        .ok()
        .map(|r| r.clone())
        .unwrap_or_default();
    let clips = crate::clip_handler::project_clips(&handle.clips, &library);
    let inbox = build_inbox(
        &handle.store,
        &handle.dismissed_episode_ids,
        &handle.inbox_triage_cache,
    );
    // Proactive triage: if any unlistened episode lacks a fresh `Ready` score,
    // spawn a background pass off the actor thread so the cache fills without
    // an explicit user `Triage` action. Cheap no-op when nothing needs triage
    // or a pass is already running (re-entrancy-guarded internally).
    if let Some(signal) = handle.snapshot_signal.clone() {
        maybe_enqueue_triage_with_signal(
            &handle.store,
            &handle.inbox_triage_cache,
            &handle.rev,
            &handle.runtime,
            &handle.inbox_triage_in_progress,
            signal,
        );
    } else {
        crate::inbox_handler::maybe_enqueue_triage(
            &handle.store,
            &handle.inbox_triage_cache,
            &handle.rev,
            &handle.runtime,
            &handle.inbox_triage_in_progress,
        );
    }
    let inbox_triage_in_progress = handle
        .inbox_triage_in_progress
        .load(std::sync::atomic::Ordering::Relaxed);
    let owned_podcasts = collect_owned_podcasts(handle);
    let downloads = handle
        .download_queue
        .lock()
        .ok()
        .and_then(|q| build_downloads_snapshot(&q));

    // Project comments for the episode the user is currently viewing
    // (set by `handle_fetch_comments`), falling back to the now-playing
    // episode when the comments section hasn't been opened this session.
    let viewed_comments_episode_id = handle
        .viewed_comments_episode_id
        .lock()
        .ok()
        .and_then(|v| v.clone());
    let comments = handle
        .comments_cache
        .lock()
        .ok()
        .and_then(|cache| {
            viewed_comments_episode_id
                .as_deref()
                .or_else(|| now_playing.as_ref().and_then(|np| np.episode_id.as_deref()))
                .and_then(|ep_id| cache.get(ep_id).cloned())
        })
        .unwrap_or_default();

    let active_account = handle.identity.lock().ok().and_then(|id| {
        id.npub.as_ref().map(|npub| AccountSummary {
            npub: npub.clone(),
            pubkey_hex: id.pubkey_hex.clone(),
            mode: "local_key".into(),
            display_name: id.display_name.clone(),
            picture_url: id.picture_url.clone(),
        })
    });

    let social = handle.social.lock().ok().and_then(|s| s.clone());

    // Feature #44 — inbound agent-to-agent kind:1 notes. Reactive push:
    // the cache is filled by `FetchAgentNotes` on the actor thread and
    // projected here on every tick (no polling, no pull symbols).
    let agent_notes = handle
        .agent_notes
        .lock()
        .ok()
        .map(|n| n.clone())
        .unwrap_or_default();

    // In-app feedback events (kind:1 + kind:513 for the TENEX project coord),
    // cached as SignedNostrEvent-shaped JSON by `FeedbackObserver`. Reactive
    // push: filled by `FetchFeedback` on the actor thread, projected here on
    // every tick (no polling, no pull symbols). The iOS `FeedbackStore` rebuilds
    // threads from this flat list.
    let feedback_events = handle
        .feedback_events_cache
        .lock()
        .ok()
        .map(|f| f.clone())
        .unwrap_or_default();

    // Configured app relays (NMP v0.2.1). Kernel-owned slot, projected by the
    // sibling helper. SAFETY: `handle.app` is the live `*mut NmpApp` the
    // host-op handler also dereferences; the actor joins before `nmp_app_free`.
    let configured_relays = unsafe { super::snapshot_relays::build_configured_relays(handle.app) };

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
        clips,
        inbox,
        inbox_triage_in_progress,
        owned_podcasts,
        downloads,
        voice,
        agent,
        agent_context,
        categories,
        social,
        agent_notes,
        configured_relays,
        feedback_events,
        ..PodcastUpdate::default()
    }
}

pub(super) fn build_snapshot_payload(handle: &PodcastHandle) -> String {
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
    if handle.is_null() {
        return 0;
    }
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
    // (which now returns `Arc::into_raw`) and has not already been freed. This
    // reclaims the shell's strong ref; the snapshot-projection closure holds a
    // second ref that is released when the app's projection registry is dropped.
    let reclaimed = unsafe { Arc::from_raw(handle as *const PodcastHandle) };
    // Fence the voice-conversation off-thread dispatch UAF: abort + join any
    // in-flight LLM turn so no spawned Tokio task can dereference `app` after
    // `nmp_app_free`. The caller contract guarantees `unregister` runs before
    // `nmp_app_free`, and (because the snapshot-projection closure holds a
    // second strong `Arc<PodcastHandle>`) the manager itself does not drop
    // here — so this explicit drain, not a `Drop` impl, is the fence.
    reclaimed.voice_conversation.shutdown();
    let _ = reclaimed.app;
}

/// Decode a binary FlatBuffers update frame into the JSON envelope consumed by
/// the iOS shell's C-string callback:
/// snapshot -> `{"t":"snapshot","v":...}`, panic -> `{"t":"panic","message":...}`.
/// Returns a heap `CString` (free with `nmp_app_free_string`) or null when the
/// frame is invalid.
///
/// # Safety
/// `bytes` must point to `len` readable bytes, or be null.
#[no_mangle]
pub unsafe extern "C" fn nmp_app_podcast_decode_update_frame(
    bytes: *const u8,
    len: usize,
) -> *mut c_char {
    if bytes.is_null() || len == 0 {
        return std::ptr::null_mut();
    }
    // SAFETY: caller guarantees `bytes` is valid for `len` bytes.
    let slice = unsafe { std::slice::from_raw_parts(bytes, len) };
    let envelope = match nmp_core::decode_update_frame(slice) {
        Ok(env) => env,
        Err(_) => return std::ptr::null_mut(),
    };
    let json = match envelope {
        nmp_core::UpdateEnvelope::Snapshot(value) => {
            serde_json::json!({ "t": "snapshot", "v": value })
        }
        nmp_core::UpdateEnvelope::Panic(panic) => {
            serde_json::json!({ "t": "panic", "message": panic.msg })
        }
    };
    match CString::new(json.to_string()) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

// Tests split into snapshot_tests.rs + snapshot_tests_ext.rs; #[path] keeps private items in scope.
#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "snapshot_tests_ext.rs"]
mod tests_ext;
