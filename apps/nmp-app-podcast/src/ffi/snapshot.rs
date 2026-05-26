//! Snapshot + unregister entry points the host calls against a
//! [`PodcastHandle`] returned by [`super::register::nmp_app_podcast_register`].
//!
//! [`PodcastUpdate`] is the typed root of the JSON the kernel emits on every
//! tick. The iOS shell decodes it via `Codable`. Fields are added milestone by
//! milestone; the empty defaults are byte-compatible with the legacy stub
//! payload (`{"running":true,"rev":0,"schema_version":1}`) so existing
//! decoders don't break before each projection's milestone wires it up.
//!
//! Per-projection field definitions live in [`super::projections`] to keep
//! this file focused on the typed root + the C-ABI entry points. Build helpers
//! for the queue, owned-podcast list, and category aggregate live in the
//! `snapshot_queue`, `snapshot_owned`, and `snapshot_categories` siblings.

use std::ffi::{c_char, CString};
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};

use super::handle::PodcastHandle;
use super::helpers::strip_html;
use super::projections::{
    AccountSummary, AgentPickSummary, AgentSnapshot, AgentTaskSummary, BriefingSnapshot,
    CategoryBrowseItem, ChapterSummary, ClipSummary, CommentSummary,
    DownloadQueueSnapshot, EpisodeSummary, InboxItem, KnowledgeSearchResult, MemoryFact,
    NostrShowSummary, OwnedPodcastInfo, PodcastSummary, SettingsSnapshot, SocialSnapshot,
    TtsEpisodeSummary, VoiceState, WidgetSnapshot, WikiArticle,
};
use super::snapshot_categories::build_category_aggregate;
use super::snapshot_owned::collect_owned_podcasts;
use super::snapshot_queue::resolve_queue_rows;
use crate::inbox_handler::build_inbox;
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
/// Typed root of the snapshot JSON. `running`, `rev`, and
/// `schema_version` mirror the kernel's existing tick contract.
/// Forward compatibility is via Swift's `Codable` tolerating unknown
/// fields; backward compatibility is gated by `schema_version` — bump
/// it only when removing or renaming a field.
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
    /// Agent-chat projection: the ordered message transcript of the
    /// active conversation plus an `is_busy` flag.
    ///
    /// `None` until the first agent turn lands during a kernel lifetime —
    /// preserves byte-identity with the legacy stub. The shape is
    /// defined alongside [`AgentSnapshot`]. The legacy multi-conversation
    /// surface lives at `super::projections::ConversationsSnapshot`
    /// (kept available as a re-export for the future
    /// `ConversationActor`-backed projection); this `agent` field is the
    /// single-thread chat surface feature #32 ships against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentSnapshot>,
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
    /// (defined alongside [`WidgetSnapshot`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<WidgetSnapshot>,
    /// Transient toast message the kernel wants the host to surface
    /// (e.g. "nothing to resume" after a Siri `Resume` with no active
    /// episode). `None` on every tick without a fresh message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toast: Option<String>,
    /// iTunes search results, populated after a `podcast.search_itunes` action.
    /// Empty when no search has been performed or after the results are consumed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub search_results: Vec<PodcastSummary>,
    /// NIP-F4 discovery results, populated after `podcast.discover_nostr`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nostr_results: Vec<NostrShowSummary>,
    /// App-settings projection (onboarding completion, auto-skip-ads, …).
    ///
    /// Defaults to the fresh-install `SettingsSnapshot`. The
    /// `skip_serializing_if = "SettingsSnapshot::is_default"` guard keeps the
    /// no-op snapshot byte-identical to the legacy stub (D6).
    #[serde(default, skip_serializing_if = "SettingsSnapshot::is_default")]
    pub settings: SettingsSnapshot,
    /// NIP-22 (kind 1111) comments for the currently-playing episode.
    ///
    /// Populated after a `podcast.fetch_comments` action lands; empty
    /// otherwise so the legacy-stub byte-identity holds for snapshots
    /// the user never asked for comments on.
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
    /// AI agent picks for the Home rail. Recomputed after every successful
    /// feed refresh and on explicit `podcast.picks.refresh` dispatches.
    /// Empty until the first refresh has run. See [`AgentPickSummary`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub picks: Vec<AgentPickSummary>,
    /// Agent-scheduled-tasks projection — see [`AgentTaskSummary`].
    /// Seeded with two defaults on first kernel launch; mutated by
    /// `podcast.tasks.*` ops. Empty vec serializes as missing for D5
    /// byte-identity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_tasks: Vec<AgentTaskSummary>,
    /// RAG / knowledge results — written by `podcast.knowledge.search`,
    /// cleared by `clear_results`. Empty preserves D5 byte-identity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub knowledge_search_results: Vec<KnowledgeSearchResult>,
    /// Agent-memory bag (feature #33). Sorted by key, empty until the user
    /// or the agent writes the first fact. Empty `Vec` is omitted from the
    /// wire payload so the legacy stub stays byte-identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memory_facts: Vec<MemoryFact>,
    /// Agent-generated TTS episode list (feature #43). Each entry is a
    /// kernel-minted [`TtsEpisodeSummary`] holding the script the voice
    /// capability speaks when the user plays it. Empty until the first
    /// `podcast.tts.generate` action; per D5/D6 the field is omitted
    /// from the wire when empty so the legacy stub stays byte-identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tts_episodes: Vec<TtsEpisodeSummary>,
    /// User-saved audio clips across all episodes. Newest-first. Empty
    /// until the first `podcast.clip.create` or `podcast.clip.auto_snip`
    /// action lands during a kernel lifetime. `episode_title` /
    /// `podcast_title` are re-joined against `PodcastStore` on every
    /// snapshot so renames flow through automatically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clips: Vec<ClipSummary>,
    /// AI-triaged inbox: every unlistened episode across the library
    /// that hasn't been dismissed, sorted highest-priority-first by
    /// [`crate::inbox_handler::build_inbox`]. Empty when there's nothing
    /// to show; omitted from the wire payload then (D5 byte-identity).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inbox: Vec<InboxItem>,
    /// User-owned podcasts (NIP-F4): rows for every podcast with a
    /// per-podcast keypair generated via `podcast.publish.create_owned_podcast`.
    /// Empty until the first `create_owned_podcast` action fires.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owned_podcasts: Vec<OwnedPodcastInfo>,
    /// Browse-by-topic aggregation surfaced via the iOS Library tab.
    ///
    /// Built by [`build_snapshot_payload`] from the kernel-side
    /// categorizer cache (`PodcastHandle::categories`) cross-referenced
    /// against the library. Empty until the first
    /// `podcast.categorize.run` action lands (auto-triggered after every
    /// successful feed refresh, so the first non-empty snapshot is the
    /// one that follows the very first subscription's refresh).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<CategoryBrowseItem>,
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
            settings: SettingsSnapshot::default(),
            comments: Vec::new(),
            queue: Vec::new(),
            wiki_articles: Vec::new(),
            wiki_search_results: Vec::new(),
            picks: Vec::new(),
            agent_tasks: Vec::new(),
            knowledge_search_results: Vec::new(),
            memory_facts: Vec::new(),
            tts_episodes: Vec::new(),
            clips: Vec::new(),
            inbox: Vec::new(),
            owned_podcasts: Vec::new(),
            categories: Vec::new(),
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
    let rev = handle.rev.load(Ordering::Relaxed);

    // Fast path: skip re-serialization when rev hasn't changed.
    if let Ok(cache) = handle.snapshot_cache.lock() {
        if let Some((cached_rev, ref cached_json)) = *cache {
            if cached_rev == rev {
                return cached_json.clone();
            }
        }
    }

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
    let inbox = build_inbox(&handle.store, &handle.dismissed_episode_ids);
    let owned_podcasts = collect_owned_podcasts(handle);

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

    let update = PodcastUpdate {
        rev,
        now_playing,
        library,
        search_results,
        nostr_results,
        settings,
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
        voice,
        agent,
        categories,
        briefing,
        ..PodcastUpdate::default()
    };
    let json = serde_json::to_string(&update)
        .unwrap_or_else(|_| r#"{"running":true,"rev":0,"schema_version":1}"#.to_owned());

    if let Ok(mut cache) = handle.snapshot_cache.lock() {
        *cache = Some((rev, json.clone()));
    }
    json
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

// Tests split into snapshot_tests.rs + snapshot_tests_ext.rs; #[path] keeps private items in scope.
#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "snapshot_tests_ext.rs"]
mod tests_ext;
