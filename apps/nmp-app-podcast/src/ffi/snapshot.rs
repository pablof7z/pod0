//! Snapshot entry points the host calls against a [`PodcastHandle`].
//!
//! [`PodcastUpdate`] type definition lives in `snapshot_update.rs`;
//! per-projection types live in [`super::projections`]; queue, owned-podcast,
//! and category build helpers live in `snapshot_queue/owned/categories` siblings.
//! The ~80-field [`SettingsSnapshot`] assembly lives in `snapshot_settings.rs`.

pub use super::snapshot_update::{AppRelayRow, PodcastUpdate};

use std::ffi::{c_char, CString};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use super::projections::{AgentSnapshot, PodcastSummary};
use super::snapshot_categories::build_category_aggregate;
use super::snapshot_downloads::build_downloads_snapshot;
use super::snapshot_owned::collect_owned_podcasts;
use super::snapshot_queue::resolve_queue_rows;
use super::snapshot_settings::build_settings_snapshot;
use super::snapshot_widget::build_widget_snapshot;
// inbox_handler imports removed in Step 7 — InboxState now owns the projection
// and the proactive trigger.  See `state::inbox::InboxState::project()` and
// `InboxState::maybe_enqueue_triage()`.

pub(super) fn provider_key_present(key: Option<&str>) -> bool {
    key.is_some_and(|value| !value.trim().is_empty())
}

/// Build the typed [`PodcastUpdate`] directly from the handle state.
///
/// Rust-native path — no JSON round-trip. Used by the TUI and other
/// Rust consumers that want typed projections without paying serde
/// serialization + deserialization.
pub fn build_podcast_update(handle: &PodcastHandle) -> PodcastUpdate {
    let rev = handle.state.infra.rev.load(Ordering::Relaxed);

    // Step 14: player_actor now sourced from state.playback.player.
    let now_playing = handle.state.playback.player.lock().ok().and_then(|a| {
        let s = a.state().clone();
        if s.episode_id.is_some() {
            Some(s)
        } else {
            None
        }
    });

    // Snapshot caches before the store lock so we don't hold two locks at once.
    // Step 5b: transcripts now read from TranscriptsState.
    let transcripts = handle.state.transcripts.snapshot();
    // Step 4: categories_cache now read from CategoriesState.
    let categories_cache = handle.state.categories.categories_snapshot();

    // Single store lock → library + memory_facts + settings.
    let (library, memory_facts, settings) = handle
        .state.library.store
        .lock()
        .ok()
        .map(|s| {
            let library = super::snapshot_library::build_library_snapshot(
                handle,
                &s,
                &transcripts,
                &categories_cache,
            );
            let settings = build_settings_snapshot(&s);
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
    // Step 9: search_results + nostr_results now read from DiscoveryState.
    let search_results = handle.state.discovery.itunes_snapshot();
    let nostr_results = handle.state.discovery.nostr_snapshot();
    // Step 14: queue now read from state.playback.queue.
    let queue_ids = handle.state.playback.queue_snapshot();
    let queue = resolve_queue_rows(&queue_ids, &library);
    // Step 3: picks slot is now owned by `state.picks`.
    let picks = handle.state.picks.picks_snapshot();
    // Step 6: agent_tasks now read from TasksState.
    let agent_tasks = handle.state.tasks.tasks_snapshot();
    let knowledge_search_results = handle.state.knowledge.results_snapshot();
    // Step 5a: clips now projected from ClipsState.
    let clips = handle.state.clips.project(&library);
    // Step 7 / D8: inbox projected from InboxState — pure, no side effects.
    // Proactive triage trigger was lifted to the feed-refresh path (Commit 2).
    let inbox = handle.state.inbox.project();
    // Step 7: inbox_triage_in_progress now read from InboxState.
    let inbox_triage_in_progress = handle.state.inbox.triage_in_progress_snapshot();
    let inbox_last_triaged_at = handle.state.inbox.last_triaged_at_snapshot();
    let owned_podcasts = collect_owned_podcasts(handle);
    // Step 14: downloads now read from state.playback.downloads.
    let downloads = handle
        .state.playback.downloads
        .lock()
        .ok()
        .and_then(|q| build_downloads_snapshot(&q));

    // Step 8: comments now read from CommentsState.
    // Project comments for the episode the user is currently viewing
    // (set by `handle_fetch_comments`), falling back to the now-playing
    // episode when the comments section hasn't been opened this session.
    let comments = handle.state.comments.project(
        now_playing.as_ref().and_then(|np| np.episode_id.as_deref()),
    );

    let active_account = super::snapshot_identity::build_active_account(handle);

    // Step 10: social now read from SocialState.
    let social = handle.state.social.social_snapshot();

    // NIP-10 threaded conversations (inbound + outbound turns merged by root_event_id).
    // The flat `agent_notes` list was retired — conversations subsume it.
    let nostr_conversations = handle.state.social.nostr_conversations_snapshot();

    // In-app feedback events (kind:1 + kind:513 for this app's project coord),
    // cached and reduced by `nmp-feedback`.
    // Step 16: feedback is now in state.feedback.
    let feedback_events = handle.state.feedback.snapshot_events();
    let feedback_threads = handle.state.feedback.snapshot_threads();

    // Configured app relays (NMP v0.2.1). Kernel-owned slot, projected by the
    // sibling helper. SAFETY: `handle.app` is the live `*mut NmpApp` the
    // host-op handler also dereferences; the actor joins before `nmp_app_free`.
    let configured_relays = unsafe { super::snapshot_relays::build_configured_relays(handle.app) };

    // Step 12: voice now projected from VoiceSubstate.
    let voice = handle.state.voice.voice_snapshot();

    // Step 11: agent chat now read from AgentChatState.
    let agent = {
        let messages = handle.state.agent_chat.conversation_snapshot();
        let touched = handle.state.agent_chat.is_touched();
        if messages.is_empty() && !touched {
            None
        } else {
            Some(AgentSnapshot {
                messages,
                is_busy: handle.state.agent_chat.is_busy(),
            })
        }
    };

    // Kernel-owned widget projection (D4 single source of truth). Built from
    // the player state + the already-assembled library (per-show
    // `unplayed_count` is reused, no rescan). The iOS shell serializes this
    // into the App Group key the widget extension reads; the old Swift-side
    // `NowPlayingSnapshot` derivation is retired.
    let widget = build_widget_snapshot(now_playing.as_ref(), &library);

    PodcastUpdate {
        rev,
        now_playing,
        library,
        widget,
        active_account,
        search_results,
        nostr_results,
        settings,
        comments,
        queue,
        picks,
        agent_tasks,
        knowledge_search_results,
        memory_facts,
        clips,
        inbox,
        inbox_triage_in_progress,
        inbox_last_triaged_at,
        owned_podcasts,
        downloads,
        voice,
        agent,
        agent_context,
        categories,
        social,
        nostr_conversations,
        configured_relays,
        feedback_events,
        feedback_threads,
        ..PodcastUpdate::default()
    }
}

pub(super) fn build_snapshot_payload(handle: &PodcastHandle) -> String {
    let rev = handle.state.infra.rev.load(Ordering::Relaxed);

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
/// **Cold-start / compat / debug only.** Normal app state updates arrive via
/// typed domain sidecars in the binary FlatBuffers push frame
/// (`nmp_app_podcast_decode_update_frame`). Do NOT call this on render ticks.
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
    ffi_guard("nmp_app_podcast_snapshot", std::ptr::null_mut, || {
        // SAFETY: caller guarantees `handle` is a valid pointer returned by
        // `nmp_app_podcast_register` and not yet freed.
        let handle = unsafe { &*handle };

        let payload = build_snapshot_payload(handle);
        let Ok(cstr) = CString::new(payload) else {
            return std::ptr::null_mut();
        };
        cstr.into_raw()
    })
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
    ffi_guard("nmp_app_podcast_snapshot_rev", || 0u64, || {
        let handle = unsafe { &*handle };
        handle.state.infra.rev.load(std::sync::atomic::Ordering::Relaxed)
    })
}

/// Free a snapshot string previously returned by [`nmp_app_podcast_snapshot`].
/// Null pointer is a silent no-op.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_snapshot_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    ffi_guard("nmp_app_podcast_snapshot_free", || (), || {
        // SAFETY: caller guarantees `ptr` came from `CString::into_raw` in
        // `nmp_app_podcast_snapshot` and has not been freed.
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    });
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
    ffi_guard("nmp_app_podcast_unregister", || (), || {
        // SAFETY: caller guarantees `handle` came from `nmp_app_podcast_register`
        // (which now returns `Arc::into_raw`) and has not already been freed. This
        // reclaims the shell's strong ref; the snapshot-projection closure holds a
        // second ref that is released when the app's projection registry is dropped.
        let reclaimed = unsafe { Arc::from_raw(handle as *const PodcastHandle) };
        // Step 12: Fence the voice-conversation off-thread dispatch UAF: abort +
        // join any in-flight LLM turn so no spawned Tokio task can dereference
        // `app` after `nmp_app_free`. The caller contract guarantees `unregister`
        // runs before `nmp_app_free`, and (because the snapshot-projection closure
        // holds a second strong `Arc<PodcastHandle>`) the manager itself does not
        // drop here — so this explicit drain, not a `Drop` impl, is the fence.
        //
        // Teardown ordering: shutdown BEFORE drop (i.e. before the `reclaimed`
        // Arc falls out of scope) — unchanged from the pre-migration fence.
        reclaimed.state.voice.shutdown();
        // Same fence for the kernel-owned task scheduler tick: abort + join the
        // periodic ticker so no spawned Tokio task can dereference `app`
        // (`nmp_app_dispatch_action`) after `nmp_app_free`.  MUST run before the
        // `reclaimed` Arc drops (i.e. before `nmp_app_free`), beside the voice
        // fence above.
        reclaimed.state.tasks.shutdown();
        let _ = reclaimed.app;
    });
}

/// Decode a binary FlatBuffers update frame into the JSON envelope consumed by
/// the iOS shell's C-string callback:
/// snapshot -> `{"t":"snapshot","v":...}`, panic -> `{"t":"panic","message":...}`.
/// Returns a heap `CString` (free with `nmp_free_string`) or null when the
/// frame is invalid.
///
/// **nmp-v0.3.0 migration note (PR-B #991/#979):** The generic `payload:Value`
/// JSON tree — which previously carried `projections["podcast.snapshot"]` and
/// `projections["signed_events"]` — is no longer present in the wire frame.
/// `UpdateEnvelope::Snapshot` now carries a typed [`SnapshotEnvelope`] (Tier-3
/// fields only: `rev`, `running`, metrics, relay statuses, error toasts).
///
/// As a result the `v` returned here no longer contains a `projections["podcast.snapshot"]`
/// entry. The iOS shell's `decodePodcastUpdate` guard falls through to the pull
/// path (`nmp_app_podcast_snapshot_rev` + `nmp_app_podcast_snapshot`), which is
/// driven by `pullPodcastSnapshotIfChanged` on every accepted push notification.
///
/// **signed_events bridge:** The `signed_events` Tier-2 typed FlatBuffer sidecar
/// (key `"signed_events"`, schema `nmp.signedEvents`) is decoded here and
/// injected under `v.projections["signed_events"]` so the iOS
/// `SignedEventsRegistry.ingest` path continues to work unchanged. Decode
/// failure degrades silently (D6 — key absent, never a crash).
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
    ffi_guard("nmp_app_podcast_decode_update_frame", std::ptr::null_mut, || {
    // SAFETY: caller guarantees `bytes` is valid for `len` bytes.
    let slice = unsafe { std::slice::from_raw_parts(bytes, len) };
    let envelope = match nmp_core::decode_update_frame(slice) {
        Ok(env) => env,
        Err(_) => return std::ptr::null_mut(),
    };
    let json = match envelope {
        // PR-B (nmp-v0.3.0): `Snapshot` now carries a typed `SnapshotEnvelope`
        // (Tier-3 fields) instead of the deleted generic `payload:Value`. Build
        // the `v` object from the available typed fields. The `projections` map
        // carries only the signed_events sidecar — `podcast.snapshot` must be
        // obtained via the pull path (`nmp_app_podcast_snapshot`), driven by
        // the shell's `pullPodcastSnapshotIfChanged` on every push frame.
        nmp_core::UpdateEnvelope::Snapshot(env) => {
            let mut v = serde_json::json!({
                "rev": env.rev,
                "running": env.running,
                "schema_version": 1u32,
            });
            // Forward the liveness / error fields the shell reads on every frame.
            if let Some(toast) = env.last_error_toast {
                v["last_error_toast"] = serde_json::Value::String(toast);
            }
            if let Some(cat) = env.last_error_category {
                v["last_error_category"] = serde_json::Value::String(cat);
            }
            // Bridge the signed_events Tier-2 typed sidecar into
            // v.projections["signed_events"] so SignedEventsRegistry.ingest
            // keeps working after the v0.3.0 typed-first migration. Decode
            // failure degrades silently (D6 — key absent, not a crash).
            //
            // Bridge the action_results Tier-2 typed sidecar into
            // v.projections["action_results"] so Swift can read the drained
            // BlobDescriptor (or any other async-completing action result)
            // keyed by correlation_id. Wire shape per action_results_fb.rs:
            //   [ { "correlation_id": "…", "status": "…", "result": "…" }, … ]
            // Decode failure degrades silently (D6).
            //
            // Also inject all podcast.* domain sidecars under
            // v.projections[key] so Swift/Android shells can consume per-domain
            // delta updates without waiting for the pull path.
            let signed_events_json = decode_signed_events_sidecar(slice);
            let action_results_json = decode_action_results_sidecar(slice);
            let domain_sidecars = super::snapshot_domain_projections::decode_podcast_domain_sidecars(slice);

            if signed_events_json.is_some() || action_results_json.is_some() || domain_sidecars.is_some() {
                let mut projections = serde_json::Map::new();
                if let Some(se) = signed_events_json {
                    projections.insert("signed_events".to_string(), se);
                }
                if let Some(ar) = action_results_json {
                    projections.insert("action_results".to_string(), ar);
                }
                if let Some(domains) = domain_sidecars {
                    for (key, val) in domains {
                        projections.insert(key, val);
                    }
                }
                v["projections"] = serde_json::Value::Object(projections);
            }
            serde_json::json!({ "t": "snapshot", "v": v })
        }
        nmp_core::UpdateEnvelope::Panic(panic) => {
            serde_json::json!({ "t": "panic", "message": panic.msg })
        }
    };
    match CString::new(json.to_string()) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
    }) // ffi_guard
}

/// Decode the `signed_events` typed FlatBuffer sidecar from a raw update-frame
/// slice and convert it to the JSON object `SignedEventsRegistry.ingest` expects:
/// `{ correlation_id: { "ok": true, "signed_json": "..." } }` or
/// `{ correlation_id: { "ok": false, "error": "..." } }`.
///
/// Returns `None` when the sidecar is absent, empty, or malformed (D6 — degrade
/// silently, never panic).
fn decode_signed_events_sidecar(slice: &[u8]) -> Option<serde_json::Value> {
    use nmp_core::typed_projections::{decode_signed_events, SIGNED_EVENTS_SCHEMA_ID};

    let typed = nmp_core::decode_snapshot_typed_projections(slice).ok()?;
    let entry = typed
        .into_iter()
        .find(|e| e.schema_id == SIGNED_EVENTS_SCHEMA_ID)?;
    let model = decode_signed_events(&entry.payload).ok()?;
    if model.entries.is_empty() {
        return None;
    }
    let mut map = serde_json::Map::with_capacity(model.entries.len());
    for (correlation_id, row) in model.entries {
        let value = if row.ok {
            serde_json::json!({ "ok": true, "signed_json": row.signed_json.unwrap_or_default() })
        } else {
            serde_json::json!({ "ok": false, "error": row.error.unwrap_or_default() })
        };
        map.insert(correlation_id, value);
    }
    Some(serde_json::Value::Object(map))
}

// Action-results sidecar decode lives in a sibling file to keep this file
// under the 500-line AGENTS.md hard limit. The function is `pub(super)` and
// visible here via the `#[path]`-linked module.
#[path = "snapshot_action_results.rs"]
mod snapshot_action_results;
use snapshot_action_results::decode_action_results_sidecar;

// Tests split into snapshot_tests.rs + snapshot_tests_ext.rs + snapshot_decode_tests.rs;
// #[path] keeps private items in scope.
#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "snapshot_tests_ext.rs"]
mod tests_ext;
#[cfg(test)]
#[path = "snapshot_decode_tests.rs"]
mod decode_tests;
