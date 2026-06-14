//! Slice-local payload builder functions for each per-domain typed projection.
//!
//! Each `build_<domain>_payload` function reads ONLY its own domain's substate
//! — no call to `build_podcast_update`.  This eliminates the full-snapshot
//! fan-in that previously ran on EVERY domain emit (including 1 Hz playback
//! ticks that were rebuilding the entire library + settings + categories on
//! every rev bump during playback).
//!
//! Per-domain substate access pattern:
//!
//!  | Domain    | Substates read                                         |
//!  |-----------|--------------------------------------------------------|
//!  | library   | library.store, transcripts, categories, discovery,     |
//!  |           | publish (owned_podcasts), inbox                        |
//!  | playback  | playback.player, playback.queue, library.store         |
//!  |           | (store is needed ONLY to resolve queue display rows)   |
//!  | downloads | playback.downloads                                     |
//!  | settings  | library.store, handle.app (relays)                    |
//!  | identity  | library.identity, handle.app (kernel active account)  |
//!  | widget    | playback.player, library.store                         |
//!  |           | (store for unplayed_count + ep title/artwork)          |
//!  | social    | social.social_slot, social.agent_notes (via convos),   |
//!  |           | social.outbound_turns                                  |
//!  | misc      | wiki, picks, tasks, knowledge, clips, comments, voice, |
//!  |           | agent_chat, feedback                                   |
//!
//! The `rev` field in each payload is the GLOBAL rev (state.infra.rev), matching
//! the pull-path `build_podcast_update` behaviour so byte identity is preserved.
//!
//! Helper functions (`build_queue_rows_from_store`, `build_widget_from_store`)
//! live in the sibling `snapshot_domain_store_helpers` module.

use std::sync::atomic::Ordering;

use super::handle::PodcastHandle;
use super::snapshot_domain_store_helpers::{
    build_queue_rows_from_store, build_widget_from_store,
};

/// Build the `podcast.library` domain payload — slice-local.
///
/// Reads: library.store (podcasts + episodes + memory_facts), transcripts
/// state, categories state, discovery state (search/nostr results), publish
/// state (owned podcasts), and inbox state.
///
/// Returns `None` when the library is empty (preserves byte-identical
/// behaviour for a fresh install).
///
/// `inbox` lives here (not in playback) because it is DERIVED from library
/// episodes — a feed refresh that adds/updates episodes drives the inbox delta,
/// and both are bumped by `Domain::Library` mutation sites.
pub(super) fn build_library_payload(handle: &PodcastHandle) -> Option<serde_json::Value> {
    use super::snapshot_categories::build_category_aggregate;
    use super::snapshot_library::build_library_snapshot;
    use super::snapshot_owned::collect_owned_podcasts;

    let rev = handle.state.infra.rev.load(Ordering::Relaxed);

    // Snapshot caches before the store lock so we don't hold two locks at once.
    let transcripts = handle.state.transcripts.snapshot();
    let categories_cache = handle.state.categories.categories_snapshot();

    // Single store lock → library + memory_facts (settings NOT read here).
    let library = handle
        .state.library.store
        .lock()
        .ok()
        .map(|s| build_library_snapshot(handle, &s, &transcripts, &categories_cache))
        .unwrap_or_default();

    if library.is_empty() {
        return None;
    }

    let subscribed_library: Vec<_> = library
        .iter()
        .filter(|p| p.is_subscribed)
        .cloned()
        .collect();
    let categories = build_category_aggregate(&subscribed_library);

    let search_results = handle.state.discovery.itunes_snapshot();
    let nostr_results = handle.state.discovery.nostr_snapshot();
    let owned_podcasts = collect_owned_podcasts(handle);
    let inbox = handle.state.inbox.project();
    let inbox_triage_in_progress = handle.state.inbox.triage_in_progress_snapshot();
    let inbox_last_triaged_at = handle.state.inbox.last_triaged_at_snapshot();

    Some(serde_json::json!({
        "rev": rev,
        "library": library,
        "categories": categories,
        "search_results": search_results,
        "nostr_results": nostr_results,
        "owned_podcasts": owned_podcasts,
        "inbox": inbox,
        "inbox_triage_in_progress": inbox_triage_in_progress,
        "inbox_last_triaged_at": inbox_last_triaged_at,
    }))
}

/// Build the `podcast.playback` domain payload — slice-local.
///
/// Reads: playback.player (now_playing), playback.queue (episode IDs), the
/// transcripts + categories caches, and library.store to resolve the queue
/// display rows. The store read is a narrow episode-by-id lookup scoped to just
/// the queued IDs — it does NOT build the full library snapshot over all
/// episodes.
///
/// Each queue row is built via the shared `episode_summary` helper (the same
/// one the library path uses per episode), so the rows are byte-identical to
/// the rows `build_podcast_update` would emit for the same queued episodes. The
/// transcripts + categories caches are snapshotted here exactly as the library
/// builder snapshots them so the per-row derived fields resolve identically.
pub(super) fn build_playback_payload(handle: &PodcastHandle) -> serde_json::Value {
    let rev = handle.state.infra.rev.load(Ordering::Relaxed);

    let now_playing = handle.state.playback.player.lock().ok().and_then(|a| {
        let s = a.state().clone();
        if s.episode_id.is_some() { Some(s) } else { None }
    });

    // Snapshot the same caches the library path uses, so queue rows resolve
    // transcript entries + AI categories identically.
    let transcripts = handle.state.transcripts.snapshot();
    let categories_cache = handle.state.categories.categories_snapshot();

    let queue_ids = handle.state.playback.queue_snapshot();
    let queue = build_queue_rows_from_store(handle, &queue_ids, &transcripts, &categories_cache);

    serde_json::json!({
        "rev": rev,
        "now_playing": now_playing,
        "queue": queue,
    })
}

/// Build the `podcast.downloads` domain payload — slice-local.
///
/// Reads: playback.downloads only.
///
/// Returns `None` when there are no active downloads (D5 — omit rather than
/// send an empty struct).
pub(super) fn build_downloads_payload(handle: &PodcastHandle) -> Option<serde_json::Value> {
    use super::snapshot_downloads::build_downloads_snapshot;

    let rev = handle.state.infra.rev.load(Ordering::Relaxed);
    let downloads = handle
        .state.playback.downloads
        .lock()
        .ok()
        .and_then(|q| build_downloads_snapshot(&q));
    downloads.map(|d| serde_json::json!({ "rev": rev, "downloads": d }))
}

/// Build the `podcast.settings` domain payload — slice-local.
///
/// Reads: library.store (settings accessors) and handle.app (configured relays
/// via the kernel relay slot). Does NOT read playback, library episodes, or
/// any per-episode state.
pub(super) fn build_settings_payload(handle: &PodcastHandle) -> serde_json::Value {
    use super::snapshot_settings::build_settings_snapshot;

    let rev = handle.state.infra.rev.load(Ordering::Relaxed);
    let settings = handle
        .state.library.store
        .lock()
        .ok()
        .map(|s| build_settings_snapshot(&s))
        .unwrap_or_default();
    let configured_relays =
        unsafe { super::snapshot_relays::build_configured_relays(handle.app) };
    serde_json::json!({
        "rev": rev,
        "settings": settings,
        "configured_relays": configured_relays,
    })
}

/// Build the `podcast.identity` domain payload — slice-local.
///
/// Reads: library.identity (local key store) and handle.app (kernel active-
/// account slot).  Returns `None` when no account is active.
pub(super) fn build_identity_payload(handle: &PodcastHandle) -> Option<serde_json::Value> {
    let rev = handle.state.infra.rev.load(Ordering::Relaxed);
    let active_account = super::snapshot_identity::build_active_account(handle);
    active_account.as_ref()?;
    Some(serde_json::json!({
        "rev": rev,
        "active_account": active_account,
    }))
}

/// Build the `podcast.widget` domain payload — slice-local.
///
/// Reads: playback.player (now_playing) and library.store (for unplayed_count
/// across subscribed shows and episode title/artwork/duration lookup).
///
/// Returns `None` when there is nothing to display (no episode loaded AND no
/// unplayed episodes across subscribed shows).
pub(super) fn build_widget_payload(handle: &PodcastHandle) -> Option<serde_json::Value> {
    let rev = handle.state.infra.rev.load(Ordering::Relaxed);

    let now_playing = handle.state.playback.player.lock().ok().and_then(|a| {
        let s = a.state().clone();
        if s.episode_id.is_some() { Some(s) } else { None }
    });

    let widget = build_widget_from_store(handle, now_playing.as_ref());
    widget.as_ref()?;
    Some(serde_json::json!({ "rev": rev, "widget": widget }))
}

/// Build the `podcast.social` domain payload — slice-local.
///
/// Reads: social.social_slot, social.agent_notes (via nostr_conversations),
/// social.outbound_turns. Does NOT touch library, playback, settings, or any
/// other substate.
///
/// Returns `None` when social AND nostr_conversations are both empty (so the
/// domain's first emit is tombstoned rather than silently absent after a
/// post-sign-out account switch that cleared the slots).
///
/// NOTE: the flat `agent_notes` field was removed in chore/retire-flat-agent-notes-projection.
/// The inbound agent-notes cache (`social.agent_notes`) is still used internally
/// by `nostr_conversations_snapshot` — only the redundant flat-list wire projection
/// was retired. Conversations already subsume it.
pub(super) fn build_social_payload(handle: &PodcastHandle) -> Option<serde_json::Value> {
    let rev = handle.state.infra.rev.load(Ordering::Relaxed);
    let social = handle.state.social.social_snapshot();
    let nostr_conversations = handle.state.social.nostr_conversations_snapshot();
    let empty = social.is_none() && nostr_conversations.is_empty();
    if empty {
        return None;
    }
    Some(serde_json::json!({
        "rev": rev,
        "social": social,
        "nostr_conversations": nostr_conversations,
    }))
}

/// Build the `podcast.misc` domain payload — slice-local.
///
/// Reads: wiki, picks, tasks, knowledge, clips, comments, voice, agent_chat,
/// feedback, and library.store (for memory_facts, clips resolution, and
/// agent_context).  Does NOT call `build_podcast_update`.
///
/// `clips.project` and `agent_context` both need the `PodcastSummary` library
/// slice, so we build it ONCE under a single store lock and reuse it for both.
/// This is still correct because `misc` domain bumps are infrequent (wiki/picks/
/// tasks/clips/agent/voice) — NOT the 1 Hz playback ticks.  The library pass
/// here is isolated to misc's own domain tick, not playback's.
///
/// `memory_facts` lives in the `PodcastStore` (keyed in `store.memory_facts`)
/// and is read here under the same store lock used for clips + agent_context.
pub(super) fn build_misc_payload(handle: &PodcastHandle) -> serde_json::Value {
    use super::agent_context::build_agent_context;
    use super::snapshot_library::build_library_snapshot;

    let rev = handle.state.infra.rev.load(Ordering::Relaxed);

    let wiki_articles = handle.state.wiki.articles_snapshot();
    let wiki_search_results = handle.state.wiki.search_results_snapshot();
    let picks = handle.state.picks.picks_snapshot();
    let agent_tasks = handle.state.tasks.tasks_snapshot();
    let knowledge_search_results = handle.state.knowledge.results_snapshot();

    // One store lock → library (for clips + agent_context) + memory_facts.
    let transcripts = handle.state.transcripts.snapshot();
    let categories_cache = handle.state.categories.categories_snapshot();
    let (library, memory_facts) = handle
        .state.library.store
        .lock()
        .ok()
        .map(|s| {
            let lib = build_library_snapshot(handle, &s, &transcripts, &categories_cache);
            let facts = s.all_memory_facts();
            (lib, facts)
        })
        .unwrap_or_else(|| (Vec::new(), Vec::new()));

    let clips = handle.state.clips.project(&library);

    let agent_context = {
        let subscribed: Vec<_> =
            library.iter().filter(|p| p.is_subscribed).cloned().collect();
        if subscribed.is_empty() {
            None
        } else {
            let now_unix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            Some(build_agent_context(&subscribed, now_unix))
        }
    };

    let comments = {
        let now_playing_ep_id = handle.state.playback.player.lock().ok().and_then(|a| {
            a.state().episode_id.clone()
        });
        handle
            .state.comments
            .project(now_playing_ep_id.as_deref())
    };
    let voice = handle.state.voice.voice_snapshot();
    let agent = {
        let messages = handle.state.agent_chat.conversation_snapshot();
        let touched = handle.state.agent_chat.is_touched();
        if messages.is_empty() && !touched {
            None
        } else {
            use super::projections::AgentSnapshot;
            Some(AgentSnapshot {
                messages,
                is_busy: handle.state.agent_chat.is_busy(),
            })
        }
    };
    let feedback_events = handle.state.feedback.snapshot_events();
    let feedback_threads = handle.state.feedback.snapshot_threads();

    serde_json::json!({
        "rev": rev,
        "wiki_articles": wiki_articles,
        "wiki_search_results": wiki_search_results,
        "picks": picks,
        "agent_tasks": agent_tasks,
        "knowledge_search_results": knowledge_search_results,
        "memory_facts": memory_facts,
        "clips": clips,
        "comments": comments,
        "voice": voice,
        "agent": agent,
        "agent_context": agent_context,
        "feedback_events": feedback_events,
        "feedback_threads": feedback_threads,
    })
}
