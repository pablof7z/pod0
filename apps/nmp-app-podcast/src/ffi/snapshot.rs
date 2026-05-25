//! Snapshot + unregister entry points the host calls against a
//! [`PodcastHandle`] returned by [`super::register::nmp_app_podcast_register`].
//!
//! ## `PodcastUpdate`
//!
//! [`PodcastUpdate`] is the typed root of the JSON the kernel emits on every
//! tick. The iOS shell decodes it via `Codable`. Fields are added milestone by
//! milestone. The struct below is the source of truth for the emitted wire
//! shape.
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
    AccountSummary, BriefingSnapshot, ChapterSummary, CommentSummary, ConversationsSnapshot,
    DownloadQueueSnapshot, EpisodeSummary, NostrShowSummary, PodcastSummary, SettingsSnapshot,
    SocialSnapshot, VoiceState, WidgetSnapshot,
    AccountSummary, BriefingSnapshot, ChapterSummary, ConversationsSnapshot, DownloadQueueSnapshot,
    EpisodeSummary, PodcastSummary, VoiceState, WidgetSnapshot, WikiArticle,
};
use super::snapshot_queue::resolve_queue_rows;
use crate::player::PlayerState;

/// Typed root of the snapshot JSON.
///
/// `running`, `rev`, and `schema_version` mirror the kernel's existing
/// tick contract. `now_playing` lands at M3.A; subsequent milestones add
/// more fields (`podcasts`, `today_queue`, `triage`, ...) as feature slices
/// land.
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
    /// Social projection: the active account's NIP-02 (kind:3) follow
    /// list, surfaced as a flat `following` list + count for the iOS
    /// "Social" tab. `None` until the NMP substrate contact store is
    /// wired into the projection layer — tracked in
    /// `docs/BACKLOG.md` (`pr-social-graph-nmp-store-wiring`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub social: Option<SocialSnapshot>,
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
    /// NIP-F4 discovery results, populated after `podcast.discover_nostr`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nostr_results: Vec<NostrShowSummary>,
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
    /// App-settings projection (onboarding completion, …).
    ///
    /// Defaults to the fresh-install `SettingsSnapshot` (`has_completed_onboarding
    /// = false`). Always emitted by the snapshot builder so iOS can read
    /// `snapshot.settings` directly without an `if let` dance; the
    /// `skip_serializing_if = "SettingsSnapshot::is_default"` guard keeps the
    /// no-op snapshot byte-identical to the legacy stub (D6).
    #[serde(default, skip_serializing_if = "SettingsSnapshot::is_default")]
    pub settings: SettingsSnapshot,
    /// NIP-22 (kind 1111) comments for the currently-playing episode.
    ///
    /// Populated after a `podcast.fetch_comments` action lands; empty
    /// otherwise so the legacy-stub byte-identity holds for snapshots
    /// the user never asked for comments on. The projection layer
    /// orders newest-first by the projection layer so the iOS shell can
    /// render the list without re-sorting.
    ///
    /// The real relay subscription wiring is deferred — see
    /// `docs/BACKLOG.md` (`pr-episode-comments-relay-wiring`). Until it
    /// lands the field stays empty even after a fetch dispatch; iOS
    /// renders the empty-state copy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<CommentSummary>,
    /// Playback "Up Next" queue, front-first. Each entry is an
    /// [`EpisodeSummary`] resolved against the library at projection time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queue: Vec<EpisodeSummary>,
    /// AI-wiki articles surfaced to the iOS reader. One entry per
    /// `(podcast_id, topic)` pair the user has asked for; the iOS
    /// `WikiView` filters down to the current show. Empty until the
    /// first `podcast.wiki.generate` lands during a kernel lifetime.
    /// Per D5 we omit the empty vec from the wire payload.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wiki_articles: Vec<WikiArticle>,
    /// Filtered result of the most recent `podcast.wiki.search` dispatch.
    /// Empty when no search is active or after the iOS shell consumes
    /// the result. Lives at the snapshot root (not inside `wiki_articles`)
    /// so the full library stays visible while a search overlay is open.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wiki_search_results: Vec<WikiArticle>,
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
            social: None,
            library: Vec::new(),
            active_account: None,
            widget: None,
            toast: None,
            search_results: Vec::new(),
            nostr_results: Vec::new(),
            queue: Vec::new(),
            settings: SettingsSnapshot::default(),
            comments: Vec::new(),
            queue: Vec::new(),
            wiki_articles: Vec::new(),
            wiki_search_results: Vec::new(),
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

    // Fast path: return the cached JSON if rev hasn't changed. This avoids
    // re-serializing the entire library on every 500ms poll when nothing
    // has changed — critical for large libraries.
    if let Ok(cache) = handle.snapshot_cache.lock() {
        if let Some((cached_rev, ref cached_json)) = *cache {
            if cached_rev == rev {
                return cached_json.clone();
            }
        }
    }

    // Single lock acquisition for both projections so the queue and
    // `now_playing` are read from the same actor state without a gap
    // a concurrent mutation could slip through.
    let (now_playing, queue) = handle.player_actor.lock().ok().map(|a| {
        let s = a.state().clone();
        let now_playing = if s.episode_id.is_some() { Some(s) } else { None };
        (now_playing, a.queue().to_vec())
    }).unwrap_or((None, Vec::new()));

    // Hold the store lock once to derive both library + settings — saves
    // a second acquisition and guarantees both projections see the same
    // store revision.
    let (library, settings) = handle
        .store
        .lock()
        .ok()
        .map(|s| {
            let library: Vec<PodcastSummary> = s
                .all_podcasts()
                .into_iter()
                .map(|(podcast, episodes)| PodcastSummary {
                    id: podcast.id.0.to_string(),
                    title: podcast.title.clone(),
                    episode_count: episodes.len(),
                    unplayed_count: 0,
                    artwork_url: podcast.image_url.as_ref().map(|u| u.to_string()),
                    feed_url: podcast.feed_url.as_ref().map(|u| u.to_string()),
                    author: if podcast.author.is_empty() {
                        None
                    } else {
                        Some(podcast.author.clone())
                    },
                    episodes: episodes
                        .iter()
                        .map(|ep| {
                            let id_str = ep.id.0.to_string();
                            let transcript = s.transcript_for(&id_str).map(str::to_owned);
                            EpisodeSummary {
                                title: ep.title.clone(),
                                podcast_id: Some(podcast.id.0.to_string()),
                                podcast_title: Some(podcast.title.clone()),
                                duration_secs: ep.duration_secs,
                                artwork_url: ep.image_url.as_ref().map(|u| u.to_string()),
                                published_at: Some(ep.pub_date.timestamp()),
                                download_path: s.local_path_for(&ep.id).map(str::to_owned),
                                description: Some(ep.description.clone()).filter(|s| !s.is_empty()),
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
                                // `position_for` already returns `None` when
                                // position == 0.0, so the projection naturally
                                // hides the field for untouched episodes.
                                playback_position_secs: s.position_for(&id_str),
                                id: id_str,
                            }
                        })
                        .collect(),
                })
                .collect();
            let settings = SettingsSnapshot {
                has_completed_onboarding: s.has_completed_onboarding(),
            };
            (library, settings)
        })
        .unwrap_or_default();
    let library = handle.store.lock().ok().map(|s| {
    let library: Vec<PodcastSummary> = handle.store.lock().ok().map(|s| {
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
                auto_download: s.is_auto_download_enabled(podcast.id),
                episodes: episodes
                    .iter()
                    .map(|ep| EpisodeSummary {
                        id: ep.id.0.to_string(),
                        title: ep.title.clone(),
                        podcast_id: Some(podcast.id.0.to_string()),
                        podcast_title: Some(podcast.title.clone()),
                        duration_secs: ep.duration_secs,
                        artwork_url: ep.image_url.as_ref().map(|u| u.to_string()),
                        published_at: Some(ep.pub_date.timestamp()),
                        download_path: s.local_path_for(&ep.id).map(str::to_owned),
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
                                            is_ai_generated: c.is_ai_generated,
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
    let nostr_results = handle.nostr_results.lock().ok()
        .map(|r| r.clone())
        .unwrap_or_default();

    let briefing = handle.briefing.lock().ok().and_then(|b| b.clone());

    let queue_ids: Vec<String> = handle.queue.lock().ok()
        .map(|q| q.items().to_vec()).unwrap_or_default();
    let queue = resolve_queue_rows(&queue_ids, &library);

    let wiki_articles = handle.wiki_articles.lock().ok()
        .map(|w| w.clone())
        .unwrap_or_default();

    let wiki_search_results = handle.wiki_search_results.lock().ok()
        .map(|w| w.clone())
        .unwrap_or_default();

    let update = PodcastUpdate {
        rev,
        now_playing,
        library,
        search_results,
        nostr_results,
        queue,
        settings,
        briefing,
        queue,
        wiki_articles,
        wiki_search_results,
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&update)
        .unwrap_or_else(|_| r#"{"running":true,"rev":0,"schema_version":1}"#.to_owned());

    // Update the cache so the next poll at the same rev skips this work.
    if let Ok(mut cache) = handle.snapshot_cache.lock() {
        *cache = Some((rev, json.clone()));
    }
    json
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

// Snapshot tests live in `snapshot_tests.rs` to keep this file under
// the 500-line hard limit (AGENTS.md). Behaviour identical — the
// `#[path]` attribute re-attaches the file as the canonical `tests`
// submodule of this module.
#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
mod tests {
    use super::*;
    use super::super::projections::{
        ContactSummary, DownloadItemSnapshot, PendingApprovalSnapshot,
    };

    #[test]
    fn default_snapshot_omits_now_playing() {
        let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
        // `skip_serializing_if = "Option::is_none"` keeps the empty
        // payload byte-identical to the legacy stub.
        assert_eq!(json, r#"{"running":true,"rev":0,"schema_version":1}"#);
        // Round-trip decode succeeds.
        let _decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
    }

    #[test]
    fn snapshot_with_now_playing_round_trips() {
        let mut state = PlayerState::idle();
        state.episode_id = Some("ep-1".into());
        state.url = Some("https://ex.com/ep-1.mp3".into());
        state.position_secs = 12.0;
        state.is_playing = true;

        let snap = PodcastUpdate {
            now_playing: Some(state.clone()),
            ..PodcastUpdate::default()
        };
        let json = serde_json::to_string(&snap).expect("encode");
        let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.now_playing, Some(state));
        assert!(decoded.running);
        assert_eq!(decoded.schema_version, 1);
    }

    #[test]
    fn snapshot_decoder_tolerates_unknown_fields() {
        // Forward-compat: an older binary decoding a newer snapshot ignores
        // fields it doesn't know about (Codable parity).
        let payload = r#"{"running":true,"rev":7,"schema_version":1,"future_field":"ignored"}"#;
        let decoded: PodcastUpdate = serde_json::from_str(payload).expect("decode");
        assert_eq!(decoded.rev, 7);
        assert!(decoded.now_playing.is_none());
        assert!(decoded.downloads.is_none());
        assert!(decoded.agent.is_none());
        assert!(decoded.voice.is_none());
        assert!(decoded.briefing.is_none());
        assert!(decoded.social.is_none());
        assert!(decoded.widget.is_none());
        assert!(decoded.toast.is_none());
    }

    #[test]
    fn snapshot_with_toast_round_trips() {
        let snap = PodcastUpdate {
            toast: Some("Nothing to resume".into()),
            ..PodcastUpdate::default()
        };
        let json = serde_json::to_string(&snap).expect("encode");
        assert!(json.contains("\"toast\":\"Nothing to resume\""));
        let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.toast, Some("Nothing to resume".to_owned()));
    }

    #[test]
    fn snapshot_omits_none_toast() {
        // D5 byte-identity: empty toast must not bloat the wire payload.
        let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
        assert!(!json.contains("toast"));
    }

    #[test]
    fn snapshot_with_widget_round_trips() {
        let widget = WidgetSnapshot {
            now_playing_episode_title: Some("Ep 42".into()),
            now_playing_podcast_title: Some("Some Show".into()),
            now_playing_artwork_url: Some("https://ex.com/art.png".into()),
            is_playing: true,
            position_fraction: 0.42,
            unplayed_count: 7,
        };
        let snap = PodcastUpdate {
            widget: Some(widget.clone()),
            ..PodcastUpdate::default()
        };
        let json = serde_json::to_string(&snap).expect("encode");
        let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.widget, Some(widget));
    }

    #[test]
    fn snapshot_with_agent_round_trips() {
        let agent = ConversationsSnapshot {
            active_count: 2,
            pending_approvals: vec![PendingApprovalSnapshot {
                id: "ap-1".into(),
                description: "publish".into(),
                requested_at: 1_700_000_000,
            }],
            latest_conversation_id: Some("conv-1".into()),
        };
        let snap = PodcastUpdate {
            agent: Some(agent.clone()),
            ..PodcastUpdate::default()
        };
        let json = serde_json::to_string(&snap).expect("encode");
        let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.agent, Some(agent));
    }

    #[test]
    fn pending_approval_snapshot_omits_unset_fields() {
        let agent = ConversationsSnapshot {
            active_count: 0,
            pending_approvals: vec![],
            latest_conversation_id: None,
        };
        let json = serde_json::to_string(&agent).expect("encode");
        // `latest_conversation_id: None` should be skipped; the other
        // fields are always present.
        assert!(!json.contains("latest_conversation_id"));
        assert!(json.contains("\"active_count\":0"));
        assert!(json.contains("\"pending_approvals\":[]"));
    }

    #[test]
    fn snapshot_with_downloads_round_trips() {
        let downloads = DownloadQueueSnapshot {
            active: vec![DownloadItemSnapshot {
                episode_id: "ep-1".into(),
                progress: 0.5,
                state: "active".into(),
                error: None,
            }],
            queued_count: 2,
            completed_today: 0,
        };
        let snap = PodcastUpdate {
            downloads: Some(downloads.clone()),
            ..PodcastUpdate::default()
        };
        let json = serde_json::to_string(&snap).expect("encode");
        let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.downloads, Some(downloads));
    }

    #[test]
    fn download_item_snapshot_omits_none_error() {
        let item = DownloadItemSnapshot {
            episode_id: "ep-1".into(),
            progress: 0.0,
            state: "queued".into(),
            error: None,
        };
        let json = serde_json::to_string(&item).expect("encode");
        assert!(!json.contains("error"));
        let decoded: DownloadItemSnapshot = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, item);
    }

    // ── Voice / briefing snapshot wiring (M8.A + M9.A) ───────────────

    #[test]
    fn snapshot_with_voice_round_trips() {
        let voice = VoiceState {
            is_speaking: true,
            current_request_id: Some("req-1".into()),
            current_voice_id: Some("rachel".into()),
        };
        let snap = PodcastUpdate {
            voice: Some(voice.clone()),
            ..PodcastUpdate::default()
        };
        let json = serde_json::to_string(&snap).expect("encode");
        let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.voice, Some(voice));
    }

    #[test]
    fn voice_state_omits_none_fields() {
        let v = VoiceState {
            is_speaking: false,
            current_request_id: None,
            current_voice_id: None,
        };
        let json = serde_json::to_string(&v).expect("encode");
        assert!(!json.contains("current_request_id"));
        assert!(!json.contains("current_voice_id"));
        assert!(json.contains("\"is_speaking\":false"));
        let decoded: VoiceState = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, v);
    }

    #[test]
    fn snapshot_with_briefing_round_trips() {
        let b = BriefingSnapshot {
            status: "generating".into(),
            segment_count: 0,
            next_scheduled_minutes: Some(45),
        };
        let snap = PodcastUpdate {
            briefing: Some(b.clone()),
            ..PodcastUpdate::default()
        };
        let json = serde_json::to_string(&snap).expect("encode");
        let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.briefing, Some(b));
    }

    #[test]
    fn briefing_snapshot_omits_none_next_scheduled() {
        let b = BriefingSnapshot {
            status: "pending".into(),
            segment_count: 0,
            next_scheduled_minutes: None,
        };
        let json = serde_json::to_string(&b).expect("encode");
        assert!(!json.contains("next_scheduled_minutes"));
        let decoded: BriefingSnapshot = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, b);
    }

    // ── Social projection wiring ─────────────────────────────────────

    #[test]
    fn snapshot_with_social_round_trips() {
        let social = SocialSnapshot {
            following: vec![ContactSummary {
                npub: "npub1aaa".into(),
                display_name: Some("Alice".into()),
                picture_url: Some("https://ex.com/a.png".into()),
            }],
            following_count: 1,
        };
        let snap = PodcastUpdate {
            social: Some(social.clone()),
            ..PodcastUpdate::default()
        };
        let json = serde_json::to_string(&snap).expect("encode");
        assert!(json.contains("\"social\""));
        let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.social, Some(social));
    }

    #[test]
    fn snapshot_omits_none_social() {
        // D5 byte-identity: a pre-fetch snapshot (no contact list yet)
        // must not bloat the wire payload with an empty `social` object.
        let json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
        assert!(!json.contains("social"));
    }
}
