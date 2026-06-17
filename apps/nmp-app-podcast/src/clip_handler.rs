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
use serde::{Deserialize, Serialize};

use crate::clip_boundaries::{
    fallback_auto_snip_bounds, resolve_auto_snip_bounds, resolve_manual_clip_bounds,
    resolve_quote_bounds, usable_clip_id, AutoSnipBounds, ClipText,
};
use crate::ffi::actions::clip_module::ClipAction;
use crate::ffi::projections::{ClipSummary, PodcastSummary};
use crate::store::PodcastStore;
use crate::store::events::{stage, EventDetail, EventSeverity};

/// Internal clip record — what the actor thread mutates.
///
/// `episode_title` / `podcast_title` are the values at create time;
/// they're only surfaced when the live library join in
/// `ffi::snapshot::project_clips` misses (episode unsubscribed).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ClipRecord {
    pub id: String,
    pub episode_id: String,
    pub episode_title: String,
    pub podcast_title: String,
    pub start_secs: f64,
    pub end_secs: f64,
    pub title: Option<String>,
    pub transcript_text: String,
    pub speaker: Option<String>,
    pub source: String,
    pub refinement_status: String,
    pub auto_snip_anchor_secs: Option<f64>,
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
                source,
                transcript_text,
                client_clip_id,
            } => self.handle_create(
                episode_id,
                start_secs,
                end_secs,
                title,
                source,
                transcript_text,
                client_clip_id,
            ),
            ClipAction::Delete { clip_id } => self.handle_delete(clip_id),
            ClipAction::AutoSnip {
                episode_id,
                position_secs,
                source,
                client_clip_id,
            } => self.handle_auto_snip(episode_id, position_secs, source, client_clip_id),
            ClipAction::ResolveQuote {
                episode_id,
                position_secs,
            } => self.handle_resolve_quote(episode_id, position_secs),
        }
    }

    fn handle_create(
        &self,
        episode_id: String,
        start_secs: f64,
        end_secs: f64,
        title: Option<String>,
        source: Option<String>,
        transcript_text: Option<String>,
        client_clip_id: Option<String>,
    ) -> serde_json::Value {
        let Some((ep_title, pod_title, _duration)) = self.episode_meta(&episode_id) else {
            return serde_json::json!({
                "ok": false,
                "error": format!("episode not found: {episode_id}")
            });
        };
        let (start, end) = normalize_range(start_secs, end_secs);
        if start < 0.0 {
            return serde_json::json!({
                "ok": false,
                "error": "clip start must be >= 0"
            });
        }
        if end <= start {
            return serde_json::json!({
                "ok": false,
                "error": "clip end must be greater than start"
            });
        }
        let transcript_bounds = self
            .timed_entries(&episode_id)
            .and_then(|entries| resolve_manual_clip_bounds(&entries, start, end, _duration));
        let start = transcript_bounds
            .as_ref()
            .map(|bounds| bounds.start_secs)
            .unwrap_or(start);
        let end = transcript_bounds
            .as_ref()
            .map(|bounds| bounds.end_secs)
            .unwrap_or(end);
        let derived = transcript_bounds
            .map(|bounds| ClipText {
                transcript_text: bounds.transcript_text,
                speaker: bounds.speaker,
            })
            .or_else(|| {
                transcript_text
                    .filter(|text| !text.trim().is_empty())
                    .map(|text| ClipText {
                        transcript_text: text,
                        speaker: None,
                    })
            });
        let id = usable_clip_id(client_clip_id);
        let source = source
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "touch".to_owned());
        let event_episode_id = episode_id.clone();
        let record = ClipRecord {
            id: id.clone(),
            episode_id,
            episode_title: ep_title,
            podcast_title: pod_title,
            start_secs: start,
            end_secs: end,
            title,
            transcript_text: derived
                .as_ref()
                .map(|text| text.transcript_text.clone())
                .unwrap_or_default(),
            speaker: derived.and_then(|text| text.speaker),
            source: source.clone(),
            refinement_status: "manual".to_owned(),
            auto_snip_anchor_secs: None,
            created_at: Utc::now().timestamp(),
        };
        match self.clips.lock() {
            Ok(mut c) => {
                c.push(record);
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "clips poisoned"}),
        }
        self.persist_clips();
        self.record_clip_created(&event_episode_id, start, end, &source);
        self.rev.fetch_add(1, Ordering::Relaxed);
        serde_json::json!({"ok": true, "clip_id": id})
    }

    fn handle_delete(&self, clip_id: String) -> serde_json::Value {
        match self.clips.lock() {
            Ok(mut c) => {
                let before = c.len();
                c.retain(|rec| rec.id != clip_id);
                if c.len() != before {
                    drop(c);
                    self.persist_clips();
                    self.rev.fetch_add(1, Ordering::Relaxed);
                }
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "clips poisoned"}),
        }
    }

    fn handle_auto_snip(
        &self,
        episode_id: String,
        position_secs: f64,
        source: Option<String>,
        client_clip_id: Option<String>,
    ) -> serde_json::Value {
        let Some((_, _, duration)) = self.episode_meta(&episode_id) else {
            return serde_json::json!({
                "ok": false,
                "error": format!("episode not found: {episode_id}")
            });
        };
        let pos = position_secs.max(0.0);
        let fallback = fallback_auto_snip_bounds(pos, duration);
        let resolved = self
            .timed_entries(&episode_id)
            .and_then(|entries| resolve_auto_snip_bounds(&entries, pos, duration));
        let bounds = resolved.unwrap_or_else(|| AutoSnipBounds {
            start_secs: fallback.0,
            end_secs: fallback.1,
            transcript_text: String::new(),
            speaker: None,
            status: "pending_transcript".to_owned(),
        });
        let start_secs = bounds.start_secs;
        let end_secs = bounds.end_secs;
        let id = usable_clip_id(client_clip_id);
        let Some((ep_title, pod_title, _)) = self.episode_meta(&episode_id) else {
            return serde_json::json!({
                "ok": false,
                "error": format!("episode not found: {episode_id}")
            });
        };
        let source = source
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "auto".to_owned());
        let event_episode_id = episode_id.clone();
        let record = ClipRecord {
            id: id.clone(),
            episode_id,
            episode_title: ep_title,
            podcast_title: pod_title,
            start_secs,
            end_secs,
            title: None,
            transcript_text: bounds.transcript_text,
            speaker: bounds.speaker,
            source: source.clone(),
            refinement_status: bounds.status,
            auto_snip_anchor_secs: Some(pos),
            created_at: Utc::now().timestamp(),
        };
        match self.clips.lock() {
            Ok(mut c) => {
                c.push(record);
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "clips poisoned"}),
        }
        self.persist_clips();
        self.record_clip_created(&event_episode_id, start_secs, end_secs, &source);
        self.rev.fetch_add(1, Ordering::Relaxed);
        serde_json::json!({"ok": true, "clip_id": id})
    }

    pub fn refine_pending_for_episode(&self, episode_id: &str) -> Vec<ClipRecord> {
        let Some((_, _, duration)) = self.episode_meta(episode_id) else {
            return Vec::new();
        };
        let Some(entries) = self.timed_entries(episode_id) else {
            return Vec::new();
        };
        let Ok(mut clips) = self.clips.lock() else {
            return Vec::new();
        };
        let mut changed = false;
        let mut refined = Vec::new();
        for rec in clips.iter_mut().filter(|rec| {
            rec.episode_id == episode_id && rec.refinement_status == "pending_transcript"
        }) {
            let Some(anchor) = rec.auto_snip_anchor_secs else {
                continue;
            };
            let Some(bounds) = resolve_auto_snip_bounds(&entries, anchor, duration) else {
                continue;
            };
            rec.start_secs = bounds.start_secs;
            rec.end_secs = bounds.end_secs;
            rec.transcript_text = bounds.transcript_text;
            rec.speaker = bounds.speaker;
            rec.refinement_status = bounds.status;
            refined.push(rec.clone());
            changed = true;
        }
        if changed {
            drop(clips);
            self.persist_clips();
            self.rev.fetch_add(1, Ordering::Relaxed);
        }
        refined
    }

    fn handle_resolve_quote(&self, episode_id: String, position_secs: f64) -> serde_json::Value {
        let Some((_, _, duration)) = self.episode_meta(&episode_id) else {
            return serde_json::json!({
                "ok": false,
                "error": format!("episode not found: {episode_id}")
            });
        };
        let Some(entries) = self.timed_entries(&episode_id) else {
            return serde_json::json!({
                "ok": false,
                "error": "timed transcript unavailable"
            });
        };
        let Some(bounds) = resolve_quote_bounds(&entries, position_secs.max(0.0), duration) else {
            return serde_json::json!({
                "ok": false,
                "error": "quote boundaries unavailable"
            });
        };
        serde_json::json!({
            "ok": true,
            "start_secs": bounds.start_secs,
            "end_secs": bounds.end_secs,
            "transcript_text": bounds.transcript_text,
            "speaker": bounds.speaker,
            "refinement_status": bounds.status,
        })
    }

    /// Look up `(episode_title, podcast_title, duration_secs)` for an
    /// episode id. Returns `None` when the episode isn't in the store.
    fn episode_meta(&self, episode_id: &str) -> Option<(String, String, Option<f64>)> {
        let store = self.store.lock().ok()?;
        store.episode_titles_and_duration(episode_id)
    }

    fn timed_entries(
        &self,
        episode_id: &str,
    ) -> Option<Vec<podcast_transcripts::TranscriptEntry>> {
        let store = self.store.lock().ok()?;
        store.timed_transcript_for(episode_id).map(|entries| entries.to_vec())
    }

    fn persist_clips(&self) {
        let Some(dir) = self
            .store
            .lock()
            .ok()
            .and_then(|store| store.data_dir().map(std::path::Path::to_path_buf))
        else {
            return;
        };
        let Ok(clips) = self.clips.lock() else {
            return;
        };
        let _ = crate::store::clip_records::save_clip_records(&dir, &clips);
    }

    fn record_clip_created(&self, episode_id: &str, start: f64, end: f64, source: &str) {
        let span = clip_span_label(start, end);
        let Ok(mut store) = self.store.lock() else {
            return;
        };
        store.emit_event(
            episode_id,
            stage::CLIP_CREATED,
            EventSeverity::Info,
            format!("Clip created · {span}"),
            vec![
                EventDetail::new("Span", span),
                EventDetail::new("Source", source.to_owned()),
            ],
        );
    }
}

fn normalize_range(a: f64, b: f64) -> (f64, f64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn clip_span_label(start: f64, end: f64) -> String {
    fn fmt(secs: f64) -> String {
        let total = secs.max(0.0).round() as i64;
        format!("{}:{:02}", total / 60, total % 60)
    }
    format!("{}–{}", fmt(start), fmt(end))
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
    let mut rows: Vec<ClipSummary> = clips
        .iter()
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
                transcript_text: rec.transcript_text.clone(),
                speaker: rec.speaker.clone(),
                source: rec.source.clone(),
                refinement_status: rec.refinement_status.clone(),
                created_at: rec.created_at,
            }
        })
        .collect();
    rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    rows
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
