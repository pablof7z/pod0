//! Clip storage + action handler — owns `Vec<ClipRecord>` and the three
//! `podcast.clip.*` ops (`create`, `delete`, `auto_snip`).
//!
//! Lives in its own module so `host_op_handler.rs` stays under the 500-LOC
//! hard ceiling. `host_op_handler` only contains a small dispatch branch
//! that delegates to `ClipHandler`.
//!
//! ## Storage model
//!
//! Clips are kept in process memory on the actor thread, behind the same
//! `Arc<Mutex<…>>` discipline as `search_results`. Persistence to disk is
//! out of scope for this PR — clips evaporate on app restart. The wire
//! shape (`ffi::projections::ClipSummary`) is what the iOS shell reads;
//! the internal `ClipRecord` captures everything the snapshot builder
//! needs to re-project on every tick.
//!
//! ## Title freshness
//!
//! `ClipRecord` stamps the episode + podcast titles at create time so
//! the iOS list can still render the row when the clip's target episode
//! has been unsubscribed (in which case the library join in
//! `ffi::snapshot::project_clips` returns nothing). When the episode is
//! still in the library, the projection re-joins against the live
//! `PodcastStore` so renames take effect immediately.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use uuid::Uuid;

use crate::ffi::actions::clip_module::ClipAction;
use crate::ffi::projections::{ClipSummary, PodcastSummary};
use crate::store::PodcastStore;

/// Internal clip record — what the actor thread mutates.
///
/// `episode_title` / `podcast_title` are the values at create time;
/// they're only surfaced when the live library join in
/// `ffi::snapshot::project_clips` misses (episode unsubscribed).
#[derive(Clone, Debug, PartialEq)]
pub struct ClipRecord {
    pub id: String,
    pub episode_id: String,
    pub episode_title: String,
    pub podcast_title: String,
    pub start_secs: f64,
    pub end_secs: f64,
    pub title: Option<String>,
    pub created_at: i64,
}

/// Per-handle clip handler. Holds shared references to the clips Vec,
/// the rev counter, and the store (for episode/podcast lookup at
/// create + auto_snip time).
pub struct ClipHandler {
    clips: Arc<Mutex<Vec<ClipRecord>>>,
    store: Arc<Mutex<PodcastStore>>,
    rev: Arc<AtomicU64>,
}

impl ClipHandler {
    pub fn new(
        clips: Arc<Mutex<Vec<ClipRecord>>>,
        store: Arc<Mutex<PodcastStore>>,
        rev: Arc<AtomicU64>,
    ) -> Self {
        Self { clips, store, rev }
    }

    /// Top-level dispatch from `host_op_handler`.
    pub fn handle(&self, action: ClipAction) -> serde_json::Value {
        match action {
            ClipAction::Create {
                episode_id,
                start_secs,
                end_secs,
                title,
            } => self.handle_create(episode_id, start_secs, end_secs, title),
            ClipAction::Delete { clip_id } => self.handle_delete(clip_id),
            ClipAction::AutoSnip {
                episode_id,
                position_secs,
            } => self.handle_auto_snip(episode_id, position_secs),
        }
    }

    fn handle_create(
        &self,
        episode_id: String,
        start_secs: f64,
        end_secs: f64,
        title: Option<String>,
    ) -> serde_json::Value {
        let Some((ep_title, pod_title, _duration)) = self.episode_meta(&episode_id) else {
            return serde_json::json!({
                "ok": false,
                "error": format!("episode not found: {episode_id}")
            });
        };
        let (start, end) = normalize_range(start_secs, end_secs);
        if !(end > start) {
            return serde_json::json!({
                "ok": false,
                "error": "clip end must be greater than start"
            });
        }
        let id = Uuid::new_v4().to_string();
        let record = ClipRecord {
            id: id.clone(),
            episode_id,
            episode_title: ep_title,
            podcast_title: pod_title,
            start_secs: start,
            end_secs: end,
            title,
            created_at: Utc::now().timestamp(),
        };
        match self.clips.lock() {
            Ok(mut c) => {
                c.push(record);
                self.rev.fetch_add(1, Ordering::Relaxed);
                serde_json::json!({"ok": true, "clip_id": id})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "clips poisoned"}),
        }
    }

    fn handle_delete(&self, clip_id: String) -> serde_json::Value {
        match self.clips.lock() {
            Ok(mut c) => {
                let before = c.len();
                c.retain(|rec| rec.id != clip_id);
                if c.len() != before {
                    self.rev.fetch_add(1, Ordering::Relaxed);
                }
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "clips poisoned"}),
        }
    }

    fn handle_auto_snip(&self, episode_id: String, position_secs: f64) -> serde_json::Value {
        let Some((_, _, duration)) = self.episode_meta(&episode_id) else {
            return serde_json::json!({
                "ok": false,
                "error": format!("episode not found: {episode_id}")
            });
        };
        let pos = position_secs.max(0.0);
        let raw_start = pos - 30.0;
        let raw_end = pos + 30.0;
        let start = raw_start.max(0.0);
        let end = match duration {
            Some(d) if d > 0.0 => raw_end.min(d),
            _ => raw_end,
        };
        self.handle_create(episode_id, start, end, None)
    }

    /// Look up `(episode_title, podcast_title, duration_secs)` for an
    /// episode id. Returns `None` when the episode isn't in the store.
    fn episode_meta(&self, episode_id: &str) -> Option<(String, String, Option<f64>)> {
        let store = self.store.lock().ok()?;
        store.episode_titles_and_duration(episode_id)
    }
}

fn normalize_range(a: f64, b: f64) -> (f64, f64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Project the in-memory `Vec<ClipRecord>` into wire-format
/// `ClipSummary` rows, re-joining against the freshly-built `library`
/// so `episode_title` and `podcast_title` reflect the latest podcast /
/// episode names rather than whatever was current at clip-create time.
/// Clips whose target episode is no longer in the library still
/// surface with the create-time titles frozen on `ClipRecord` so the
/// iOS list can render them rather than silently dropping the row.
///
/// Ordering: newest-first (reverse insertion order). The iOS shell
/// renders the list as-given.
pub(crate) fn project_clips(
    clips: &Mutex<Vec<ClipRecord>>,
    library: &[PodcastSummary],
) -> Vec<ClipSummary> {
    let Ok(clips) = clips.lock() else {
        return Vec::new();
    };
    clips
        .iter()
        .rev()
        .map(|rec| {
            let (ep_title, pod_title) = lookup_titles(library, &rec.episode_id)
                .unwrap_or_else(|| (rec.episode_title.clone(), rec.podcast_title.clone()));
            ClipSummary {
                id: rec.id.clone(),
                episode_id: rec.episode_id.clone(),
                episode_title: ep_title,
                podcast_title: pod_title,
                start_secs: rec.start_secs,
                end_secs: rec.end_secs,
                title: rec.title.clone(),
                created_at: rec.created_at,
            }
        })
        .collect()
}

fn lookup_titles(library: &[PodcastSummary], episode_id: &str) -> Option<(String, String)> {
    for show in library {
        if let Some(ep) = show.episodes.iter().find(|e| e.id == episode_id) {
            return Some((ep.title.clone(), show.title.clone()));
        }
    }
    None
}


// Tests split into clip_handler_tests.rs; #[path] keeps private items in scope.
#[cfg(test)]
#[path = "clip_handler_tests.rs"]
mod tests;
