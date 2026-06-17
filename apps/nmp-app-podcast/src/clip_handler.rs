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
//! `Arc<Mutex<…>>` discipline as `search_results`. After every create /
//! delete the handler writes through to `PodcastStore::set_clips` which
//! atomically persists the clip list to `podcasts.json` — clips survive
//! app restart (D0). The wire shape (`ffi::projections::ClipSummary`) is
//! what the iOS shell reads; the internal `ClipRecord` captures everything
//! the snapshot builder needs to re-project on every tick.
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

use podcast_core::Chapter;
use podcast_transcripts::TranscriptEntry;

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
        if end <= start {
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
                let snapshot = c.clone();
                drop(c); // release clips lock before acquiring store lock
                self.persist_clips(snapshot);
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
                    let snapshot = c.clone();
                    drop(c); // release clips lock before acquiring store lock
                    self.persist_clips(snapshot);
                } else {
                    drop(c);
                }
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "clips poisoned"}),
        }
    }

    /// Persist `clips` snapshot to disk via the store's atomic write path.
    ///
    /// Lock ordering: clips lock must be **released** before calling this
    /// (callers `drop(c)` first) to avoid a potential inversion with the
    /// store's own Mutex.
    fn persist_clips(&self, clips: Vec<ClipRecord>) {
        if let Ok(mut store) = self.store.lock() {
            store.set_clips(clips);
        }
        // Silently degrade on poison — the in-memory slot stays authoritative (D6).
    }

    /// Transcript-refined, chapter-snapped AutoSnip.
    ///
    /// Pipeline (in priority order):
    /// 1. **Chapter snap** — resolve `position_secs` to the enclosing chapter's
    ///    `[start, end)` using `chapter_snap`.
    /// 2. **Transcript refine** — when a timed transcript exists, pass the
    ///    chapter-derived range through `transcript_refine` to snap start
    ///    backward to a segment boundary and end forward to a segment end.
    ///    When refine returns `None` (degenerate or no entries) keep the chapter
    ///    range.
    /// 3. **Fallback** — when no chapter range can be produced, use the legacy
    ///    ±30 s window (S2 behaviour, unchanged).
    ///
    /// No wire shape change — result is forwarded to `handle_create`.
    fn handle_auto_snip(&self, episode_id: String, position_secs: f64) -> serde_json::Value {
        let Some((_, _, chapters, duration, timed_entries)) =
            self.episode_snip_context(&episode_id)
        else {
            return serde_json::json!({
                "ok": false,
                "error": format!("episode not found: {episode_id}")
            });
        };

        let (cs_start, cs_end, chapter_title) =
            chapter_snap(position_secs, chapters.as_deref(), duration);

        // Transcript-refine post-pass: snap to nearest utterance boundaries.
        // Falls back to the chapter/±30 s range when absent or degenerate.
        let (start, end) =
            match transcript_refine(cs_start, cs_end, timed_entries.as_deref(), duration) {
                Some((rs, re)) => (rs, re),
                None => (cs_start, cs_end),
            };

        self.handle_create(episode_id, start, end, chapter_title)
    }

    /// Look up `(episode_title, podcast_title, duration_secs)` for an
    /// episode id. Returns `None` when the episode isn't in the store.
    fn episode_meta(&self, episode_id: &str) -> Option<(String, String, Option<f64>)> {
        let store = self.store.lock().ok()?;
        store.episode_titles_and_duration(episode_id)
    }

    /// Fetch all AutoSnip context in one lock acquisition:
    /// `(episode_title, podcast_title, chapters, duration_secs, timed_entries)`.
    ///
    /// Single store-lock acquisition — handler drops it before clips work.
    fn episode_snip_context(
        &self,
        episode_id: &str,
    ) -> Option<(String, String, Option<Vec<Chapter>>, Option<f64>, Option<Vec<TranscriptEntry>>)>
    {
        let store = self.store.lock().ok()?;
        store.episode_auto_snip_context(episode_id)
    }
}

/// Snap `[start, end]` to the nearest transcript-entry boundaries.
///
/// - **START (backward bias):** last entry whose `start_secs <= start`; if
///   before the first entry, snap to `entries[0].start_secs`.
/// - **END (forward bias):** first entry whose `end_secs >= end`; if past
///   the last entry, snap to `entries.last().end_secs`.
/// - Both clamped to `[0, duration]` when duration is known.
/// - Returns `None` for `None`/empty entries or a degenerate range after
///   snapping — caller keeps the pre-refine (chapter/±30 s) range.
pub(crate) fn transcript_refine(
    start: f64,
    end: f64,
    entries: Option<&[TranscriptEntry]>,
    duration: Option<f64>,
) -> Option<(f64, f64)> {
    let entries = entries?;
    if entries.is_empty() {
        return None;
    }

    // Stable sort by start_secs (same pattern as chapter_snap_range).
    let mut sorted: Vec<&TranscriptEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| {
        a.start_secs
            .partial_cmp(&b.start_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // START: last entry starting at/before `start` (backward bias).
    let snapped_start = if start < sorted[0].start_secs {
        sorted[0].start_secs
    } else {
        let mut best = sorted[0];
        for e in &sorted {
            if e.start_secs <= start {
                best = e;
            } else {
                break;
            }
        }
        best.start_secs
    };

    // END: first entry ending at/after `end` (forward bias).
    let last = sorted[sorted.len() - 1];
    let snapped_end = if end >= last.end_secs {
        last.end_secs
    } else if end < sorted[0].start_secs {
        sorted[0].end_secs
    } else {
        let mut found = last;
        for e in &sorted {
            if e.end_secs >= end {
                found = e;
                break;
            }
        }
        found.end_secs
    };

    let snapped_start = clamp_duration(snapped_start.max(0.0), duration);
    let snapped_end = clamp_duration(snapped_end.max(0.0), duration);

    if snapped_end <= snapped_start {
        return None;
    }

    Some((snapped_start, snapped_end))
}

fn normalize_range(a: f64, b: f64) -> (f64, f64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Compute `(start, end, title)` for an AutoSnip at `position_secs`.
///
/// ## Chapter path
///
/// When `chapters` is `Some` and non-empty the chapters are sorted by
/// `start_secs` and the one whose half-open interval
/// `[chapter.start, next_chapter.start)` contains `position_secs` is
/// selected. Edge cases:
///
/// - `pos` before the first chapter's start → `[0.0, first.start_secs]`
///   (or `[0.0, duration]` if the first chapter starts at 0).
/// - `pos` inside the last chapter → `[last.start_secs, duration]`
///   (or unclamped `last.start_secs + 30.0` when duration is unknown).
/// - `pos` past duration → same as last chapter rule, clamped by duration.
///
/// The chapter's `title` is returned so the `ClipRecord` can surface it
/// as the clip title (optional nice-to-have; nil when no match).
///
/// ## Fallback path
///
/// The legacy ±30 s window (clamped to `[0, duration]` when known) is used
/// whenever a chapter-derived range cannot be produced:
/// - `chapters` is `None` or empty, OR
/// - the chosen chapter range is **degenerate** (`end <= start`). This can
///   happen when the last chapter's `start_secs == duration` (a chapter that
///   begins exactly at episode end → `[duration, duration]`) or when two
///   chapters share a start (a zero-length interval). Emitting such a range
///   would make `handle_create` reject the clip ("end must be greater than
///   start"), silently failing the AutoSnip — so we fall back to a usable
///   window instead.
pub(crate) fn chapter_snap(
    position_secs: f64,
    chapters: Option<&[Chapter]>,
    duration: Option<f64>,
) -> (f64, f64, Option<String>) {
    let pos = position_secs.max(0.0);

    // Try the chapter path first; it yields `Some` only when it produces a
    // non-degenerate `(start, end)` (`end > start`). Any degenerate result
    // falls through to the ±30 s window below so AutoSnip always emits a
    // usable clip.
    if let Some(snap) = chapter_snap_range(pos, chapters, duration) {
        return snap;
    }

    // Fallback: ±30 s window clamped to [0, duration].
    let raw_start = pos - 30.0;
    let raw_end = pos + 30.0;
    let start = raw_start.max(0.0);
    let end = match duration {
        Some(d) if d > 0.0 => raw_end.min(d),
        _ => raw_end,
    };
    (start, end, None)
}

/// Resolve `pos` to a chapter-snapped `(start, end, title)`, or `None` when no
/// usable (non-degenerate) chapter range exists. Kept separate from
/// [`chapter_snap`] so the degenerate-range guard lives in one place and the
/// caller can cleanly fall through to the ±30 s window.
fn chapter_snap_range(
    pos: f64,
    chapters: Option<&[Chapter]>,
    duration: Option<f64>,
) -> Option<(f64, f64, Option<String>)> {
    let chs = chapters?;
    if chs.is_empty() {
        return None;
    }

    // Sort chapters by start time for reliable interval arithmetic. Stable
    // sort keeps duplicate-start chapters in their original order so the
    // result is deterministic.
    let mut sorted: Vec<&Chapter> = chs.iter().collect();
    sorted.sort_by(|a, b| {
        a.start_secs
            .partial_cmp(&b.start_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // pos before the first chapter's start → pre-chapter segment
    // [0, first.start_secs]. (When first.start_secs == 0 this is degenerate
    // and the guard below returns None → ±30 s fallback.)
    if pos < sorted[0].start_secs {
        let end = clamp_duration(sorted[0].start_secs, duration);
        return non_degenerate(0.0, end, None);
    }

    // Find the chapter containing pos under half-open [start, next_start)
    // semantics: a pos exactly on a boundary belongs to the chapter that
    // *starts* at it. We therefore advance past any chapter whose end is
    // <= pos, landing on the one whose interval contains pos (or the last).
    for (i, ch) in sorted.iter().enumerate() {
        let next_start = sorted.get(i + 1).map(|n| n.start_secs);
        let ch_end = next_start.or(duration).unwrap_or(ch.start_secs + 30.0);

        // `pos < ch_end` → strictly inside this chapter (half-open).
        // `next_start.is_none()` → last chapter: it owns everything from its
        // start onward, including pos == ch_end.
        if pos < ch_end || next_start.is_none() {
            let start = ch.start_secs;
            let end = clamp_duration(ch_end, duration);
            return non_degenerate(start, end, Some(ch.title.clone()));
        }
    }

    None
}

/// Return `Some((start, end, title))` only when the range is usable
/// (`end > start`); otherwise `None` so the caller falls back to the ±30 s
/// window. Centralizes the degenerate-range guard (FIX 1).
#[inline]
fn non_degenerate(
    start: f64,
    end: f64,
    title: Option<String>,
) -> Option<(f64, f64, Option<String>)> {
    if end > start {
        Some((start, end, title))
    } else {
        None
    }
}

/// Clamp `value` to at most `duration` when duration is known and positive.
#[inline]
fn clamp_duration(value: f64, duration: Option<f64>) -> f64 {
    match duration {
        Some(d) if d > 0.0 => value.min(d),
        _ => value,
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
