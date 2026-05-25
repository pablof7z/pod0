//! Snapshot + unregister entry points the host calls against a
//! [`PodcastHandle`] returned by [`super::register::nmp_app_podcast_register`].
//!
//! ## `PodcastUpdate`
//!
//! [`PodcastUpdate`] is the typed root of the JSON the kernel emits on every
//! tick. The iOS shell decodes it via `Codable`. Fields are added milestone by
//! milestone (see `Plans/nmp-migration/04-snapshot.md` for the full target
//! shape).
//!
//! For M3.A the only new field is `now_playing: Option<PlayerState>`. M4.A
//! adds `downloads: Option<DownloadQueueSnapshot>`. M7.A adds
//! `agent: Option<ConversationsSnapshot>`. M8.A adds
//! `voice: Option<VoiceState>`. M9.A adds
//! `briefing: Option<BriefingSnapshot>`. M11 adds
//! `widget: Option<WidgetSnapshot>`. Every other field stays unset until
//! its milestone lands — the empty defaults are deliberately byte-compatible
//! with the legacy stub payload (`{"running":true,"rev":0,"schema_version":1}`)
//! so existing decoders don't break before each projection's milestone wires
//! it up.
//!
//! Per-projection field definitions live in [`super::projections`] to keep
//! this file focused on the typed root + the C-ABI entry points.

use std::ffi::{c_char, CString};

use serde::{Deserialize, Serialize};

use super::handle::PodcastHandle;
use std::sync::atomic::Ordering;

use super::projections::{
    AccountSummary, BriefingSnapshot, ChapterSummary, ConversationsSnapshot, DownloadQueueSnapshot,
    EpisodeSummary, PodcastSummary, VoiceState, WidgetSnapshot,
};
use crate::player::PlayerState;

/// Typed root of the snapshot JSON.
///
/// `running`, `rev`, and `schema_version` mirror the kernel's existing
/// tick contract. `now_playing` lands at M3.A; subsequent milestones add
/// more fields (`podcasts`, `today_queue`, `triage`, …) per
/// `Plans/nmp-migration/04-snapshot.md`.
///
/// Forward compatibility: Swift's `Codable` round-trip tolerates unknown
/// fields, so introducing a new field here only needs a matching Swift
/// decoder. **Backward** compatibility (older binaries decoding a newer
/// snapshot) is the contract behind `schema_version`; bump it only when
/// removing or renaming a field.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PodcastUpdate {
    /// `true` once the kernel is running. False during shutdown.
    pub running: bool,
    /// Monotonically increasing revision id; iOS uses it to dedupe ticks.
    pub rev: u64,
    /// Schema version — bump on incompatible shape changes.
    pub schema_version: u32,
    /// Active player projection, or `None` when nothing is loaded.
    ///
    /// Per D5 the field is `null` when no episode is loaded so the
    /// iOS decoder doesn't render a hero with default zeros.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now_playing: Option<PlayerState>,
    /// Active download-queue projection, or `None` when no downloads
    /// have ever been enqueued during this kernel lifetime.
    ///
    /// Per D5 we serialize `None` (not an empty struct) when there is
    /// nothing to show — keeps the byte-compatible legacy stub for
    /// "no-op snapshot" intact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<DownloadQueueSnapshot>,
    /// Agent-chat projection: active conversation count + pending
    /// approvals queue + the most recently touched conversation id.
    ///
    /// `None` until the first agent turn lands during a kernel
    /// lifetime — preserves byte-identity with the legacy stub.
    /// The shape is defined alongside [`ConversationsSnapshot`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<ConversationsSnapshot>,
    /// Voice projection: whether TTS is currently speaking and (when
    /// it is) the in-flight request id + active voice id.
    ///
    /// `None` while no voice session is active — preserves byte-
    /// identity with the legacy stub for non-voice-mode snapshots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<VoiceState>,
    /// Briefing projection: lifecycle status of the current briefing
    /// (if any) + segment count + minutes until the next scheduled
    /// slot. `None` when the scheduler has never been touched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub briefing: Option<BriefingSnapshot>,
    /// Subscribed-podcast library projection. Each entry is a narrow
    /// [`PodcastSummary`] with embedded episode rows (newest-first).
    /// Empty until the first successful `podcast.subscribe` action.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub library: Vec<PodcastSummary>,
    /// Active Nostr identity, or `None` when no account is loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_account: Option<AccountSummary>,
    /// Platform-integration projection: the narrow slice the iOS
    /// widget extension, Live Activity, Handoff, and Siri-shortcut
    /// executors need to render "now playing" + queue summary
    /// without re-reading the rest of the snapshot.
    ///
    /// `None` until the M11 platform capability lands; the field
    /// is the kernel's policy hand-off to the host (D7 — Rust
    /// decides *what* the widget surfaces; iOS only serializes).
    /// Defined alongside [`WidgetSnapshot`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<WidgetSnapshot>,
    /// Transient toast message the kernel wants the host to surface
    /// (e.g. "nothing to resume" after a Siri `Resume` with no active
    /// episode — see `ffi::actions::SiriResumeAction` doc-comment).
    ///
    /// `None` on every tick that doesn't have a fresh message;
    /// preserves byte-identity with the legacy stub. The host clears
    /// its surfaced banner when the field flips back to `None`.
    /// Per D7 the kernel decides whether to emit a toast; the host
    /// only renders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toast: Option<String>,
    /// iTunes search results, populated after a `podcast.search_itunes` action.
    /// Empty when no search has been performed or after the results are consumed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub search_results: Vec<PodcastSummary>,
    /// Playback queue ("Up Next") — ordered list of episode ids the
    /// player will pick up after `now_playing` finishes (manually via
    /// `play_next`, or on natural completion once auto-advance lands).
    ///
    /// Lives at the snapshot root, not inside [`PlayerState`], so the
    /// queue stays visible even when `now_playing` is `None` (e.g.
    /// before the first `play` action). Per D5 we serialize an empty
    /// vec only by omitting it from the wire payload, preserving
    /// byte-identity with the legacy stub.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queue: Vec<String>,
}

impl Default for PodcastUpdate {
    fn default() -> Self {
        Self {
            running: true,
            rev: 0,
            schema_version: 1,
            now_playing: None,
            downloads: None,
            agent: None,
            voice: None,
            briefing: None,
            library: Vec::new(),
            active_account: None,
            widget: None,
            toast: None,
            search_results: Vec::new(),
            queue: Vec::new(),
        }
    }
}

/// Build the JSON payload for one snapshot tick.
///
/// Reads `player_actor`, `store`, and `rev` from `handle` under their
/// respective short-duration locks, assembles the typed [`PodcastUpdate`],
/// and serializes it. Failures degrade to the byte-compatible legacy stub
/// (D6).
fn build_snapshot_payload(handle: &PodcastHandle) -> String {
    // Read rev without modifying it — writes bump rev in PodcastHostOpHandler.
    let rev = handle.rev.load(Ordering::Relaxed);

    // Single lock acquisition for both projections so the queue and
    // `now_playing` are read from the same actor state without a gap
    // a concurrent mutation could slip through.
    let (now_playing, queue) = handle.player_actor.lock().ok().map(|a| {
        let s = a.state().clone();
        let now_playing = if s.episode_id.is_some() { Some(s) } else { None };
        (now_playing, a.queue().to_vec())
    }).unwrap_or((None, Vec::new()));

    let library = handle.store.lock().ok().map(|s| {
        s.all_podcasts()
            .into_iter()
            .map(|(podcast, episodes)| PodcastSummary {
                id: podcast.id.0.to_string(),
                title: podcast.title.clone(),
                episode_count: episodes.len(),
                unplayed_count: 0,
                artwork_url: podcast.image_url.as_ref().map(|u| u.to_string()),
                feed_url: podcast.feed_url.as_ref().map(|u| u.to_string()),
                author: if podcast.author.is_empty() { None } else { Some(podcast.author.clone()) },
                episodes: episodes
                    .iter()
                    .map(|ep| {
                        let ep_id = ep.id.0.to_string();
                        let transcript = s.transcript_for(&ep_id).map(str::to_owned);
                        EpisodeSummary {
                            id: ep_id,
                            title: ep.title.clone(),
                            podcast_id: Some(podcast.id.0.to_string()),
                            podcast_title: Some(podcast.title.clone()),
                            duration_secs: ep.duration_secs,
                            artwork_url: ep.image_url.as_ref().map(|u| u.to_string()),
                            published_at: Some(ep.pub_date.timestamp()),
                            download_path: s.local_path_for(&ep.id).map(str::to_owned),
                            transcript,
                            chapters: ep
                                .chapters
                                .as_ref()
                                .map(|cs| {
                                    cs.iter()
                                        .map(|c| ChapterSummary {
                                            start_secs: c.start_secs,
                                            end_secs: c.end_secs,
                                            title: c.title.clone(),
                                            image_url: c.image_url.as_ref().map(|u| u.to_string()),
                                            url: c.link_url.as_ref().map(|u| u.to_string()),
                                        })
                                        .collect()
                                })
                                .unwrap_or_default(),
                        }
                    })
                    .collect(),
            })
            .collect()
    }).unwrap_or_default();

    let search_results = handle.search_results.lock().ok()
        .map(|r| r.clone())
        .unwrap_or_default();

    let update = PodcastUpdate {
        rev,
        now_playing,
        library,
        search_results,
        queue,
        ..PodcastUpdate::default()
    };
    serde_json::to_string(&update)
        .unwrap_or_else(|_| r#"{"running":true,"rev":0,"schema_version":1}"#.to_owned())
}

/// Serialize the current app state into a JSON C string.
///
/// Returns null on any failure (null handle, `CString` nul-byte conflict).
/// The returned pointer is owned by the caller; pass it to
/// [`nmp_app_podcast_snapshot_free`] when done.
///
/// The payload shape is defined by [`PodcastUpdate`]; new projections are
/// added milestone by milestone (M3.A adds `now_playing`; subsequent
/// milestones wire `podcasts`, `today_queue`, `triage`, …).
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
    // Future milestones will use `boxed.app` to call
    // `app_ref.unregister_event_observer(observer_id)` for each registered
    // projection. For now the handle carries the `app` pointer so subsequent
    // milestones can add unregister logic here without changing the FFI type.
    let _ = boxed.app;
    // boxed dropped here.
}
