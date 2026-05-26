//! Picks-projection compute + slot writeback for the `podcast.picks` action
//! namespace.
//!
//! Extracted into its own file so `host_op_handler.rs` stays under the 500-line
//! hard cap and so the heuristic-vs-future-LLM swap-out point is obvious.
//!
//! The store→candidate translation lives here (not in `picks_module.rs`) so the
//! pure heuristic stays decoupled from `PodcastStore` internals; this file is
//! the only consumer that knows how to walk the store.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::ffi::actions::picks_module::{compute_picks, CandidateEpisode};
use crate::ffi::projections::AgentPickSummary;
use crate::store::PodcastStore;

/// Recompute the picks slot from the current `PodcastStore` contents,
/// stamp it onto the shared `picks_slot`, and bump `rev` so the next
/// iOS snapshot poll observes the change.
///
/// Lock discipline: the store is locked only long enough to drain it
/// into a flat `Vec<CandidateEpisode>`. The picks slot is then locked
/// separately for the write — never both at once. Failure (poisoned
/// locks) degrades silently per D6.
pub fn refresh_picks_into_slot(
    store: &Arc<Mutex<PodcastStore>>,
    picks_slot: &Arc<Mutex<Vec<AgentPickSummary>>>,
    rev: &Arc<AtomicU64>,
) {
    let candidates = match store.lock() {
        Ok(s) => collect_candidates(&s),
        Err(_) => return,
    };
    let picks = compute_picks(candidates);
    if let Ok(mut slot) = picks_slot.lock() {
        *slot = picks;
        rev.fetch_add(1, Ordering::Relaxed);
    }
}

/// Flatten the store into the heuristic's input shape.
///
/// Iterates every subscribed podcast + every episode. The heuristic
/// itself decides ordering + caps; we just hand it the raw set.
fn collect_candidates(store: &PodcastStore) -> Vec<CandidateEpisode> {
    let mut out: Vec<CandidateEpisode> = Vec::new();
    for (podcast, episodes) in store.all_podcasts() {
        let podcast_id = podcast.id.0.to_string();
        let podcast_title = podcast.title.clone();
        let show_art = podcast.image_url.as_ref().map(|u| u.to_string());
        for ep in episodes {
            let ep_art = ep
                .image_url
                .as_ref()
                .map(|u| u.to_string())
                .or_else(|| show_art.clone());
            out.push(CandidateEpisode {
                episode_id: ep.id.0.to_string(),
                episode_title: ep.title.clone(),
                podcast_id: podcast_id.clone(),
                podcast_title: podcast_title.clone(),
                artwork_url: ep_art,
                published_at: ep.pub_date.timestamp(),
                duration_secs: ep.duration_secs,
            });
        }
    }
    out
}

/// Handler for `{"op":"refresh"}` on the `podcast.picks` namespace.
///
/// Wraps [`refresh_picks_into_slot`] in the `{"ok":true}` envelope every
/// host-op handler returns. Kept here (not in `host_op_handler.rs`) so the
/// host op file stays under the 500-line cap.
pub fn handle_refresh(
    store: &Arc<Mutex<PodcastStore>>,
    picks_slot: &Arc<Mutex<Vec<AgentPickSummary>>>,
    rev: &Arc<AtomicU64>,
) -> serde_json::Value {
    refresh_picks_into_slot(store, picks_slot, rev);
    serde_json::json!({"ok": true})
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use podcast_core::{Episode, Podcast, PodcastId};
    use url::Url;
    use uuid::Uuid;

    fn make_podcast(title: &str) -> Podcast {
        Podcast::new(title)
    }

    fn make_episode(podcast_id: PodcastId, title: &str, ts: i64) -> Episode {
        Episode::new(
            podcast_id,
            "https://example.com/feed.xml",
            format!("guid-{}", Uuid::new_v4()),
            title,
            Url::parse("https://example.com/audio.mp3").unwrap(),
            Utc.timestamp_opt(ts, 0).single().unwrap(),
        )
    }

    #[test]
    fn collect_candidates_returns_all_episodes() {
        let mut store = PodcastStore::new();
        let p1 = make_podcast("Show A");
        let p1_id = p1.id;
        let p2 = make_podcast("Show B");
        let p2_id = p2.id;
        store.subscribe(p1, vec![
            make_episode(p1_id, "A-1", 100),
            make_episode(p1_id, "A-2", 200),
        ]);
        store.subscribe(p2, vec![make_episode(p2_id, "B-1", 300)]);

        let cands = collect_candidates(&store);
        assert_eq!(cands.len(), 3);
        // Show titles come through.
        let titles: std::collections::HashSet<&str> =
            cands.iter().map(|c| c.podcast_title.as_str()).collect();
        assert!(titles.contains("Show A"));
        assert!(titles.contains("Show B"));
    }

    #[test]
    fn refresh_picks_writes_into_slot_and_bumps_rev() {
        let mut s = PodcastStore::new();
        let p = make_podcast("Refresh Show");
        let pid = p.id;
        s.subscribe(p, vec![make_episode(pid, "ep-1", 100)]);
        let store = Arc::new(Mutex::new(s));
        let slot = Arc::new(Mutex::new(Vec::<AgentPickSummary>::new()));
        let rev = Arc::new(AtomicU64::new(0));

        refresh_picks_into_slot(&store, &slot, &rev);
        let written = slot.lock().unwrap();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].podcast_title, "Refresh Show");
        assert_eq!(rev.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn handle_refresh_returns_ok_envelope_and_populates_slot() {
        let mut s = PodcastStore::new();
        let p = make_podcast("Envelope Show");
        let pid = p.id;
        s.subscribe(p, vec![make_episode(pid, "ep-1", 100)]);
        let store = Arc::new(Mutex::new(s));
        let slot = Arc::new(Mutex::new(Vec::<AgentPickSummary>::new()));
        let rev = Arc::new(AtomicU64::new(0));

        let resp = handle_refresh(&store, &slot, &rev);
        assert_eq!(resp["ok"], true);
        assert_eq!(slot.lock().unwrap().len(), 1);
    }

    #[test]
    fn refresh_picks_on_empty_store_yields_empty_slot() {
        let store = Arc::new(Mutex::new(PodcastStore::new()));
        let slot = Arc::new(Mutex::new(Vec::<AgentPickSummary>::new()));
        let rev = Arc::new(AtomicU64::new(0));
        refresh_picks_into_slot(&store, &slot, &rev);
        assert!(slot.lock().unwrap().is_empty());
        // Slot rev still bumps — keeps the iOS poll loop simple.
        assert_eq!(rev.load(Ordering::Relaxed), 1);
    }
}
