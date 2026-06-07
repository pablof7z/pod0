//! Per-episode pipeline event log — the kernel's single source of truth for
//! "what has the processing pipeline done to this episode?".
//!
//! The Diagnostics sheet in iOS answers user-facing questions like "why does
//! this episode have no transcript?" and "did it even try to download?". To
//! answer those honestly every pipeline transition — download requested /
//! started / finished / failed, transcript queued / fetching / ready / failed,
//! chapter + ad identification — emits an [`EpisodeEvent`] here. iOS reads them
//! lazily through `nmp_app_podcast_episode_events` when the sheet opens; the
//! events deliberately do **not** ride the library snapshot (which is fully
//! JSON-decoded on the main thread on every `rev` bump) nor the wholesale
//! `podcasts.json` persist path (which rewrites the entire library on every
//! download completion). Instead each episode owns a small JSON file under
//! `data_dir/episode-events/<episode_id>.json`, written on append.
//!
//! Wire shape is pinned to the Swift `EpisodeAuditEvent` decoder
//! (`App/Sources/Podcast/EpisodeAuditEvent.swift`): `episodeID` is camelCase,
//! `timestamp` is RFC3339 **without** fractional seconds and with a `Z` suffix
//! (Swift's `.iso8601` strategy rejects fractional seconds), and `kind` carries
//! the same dotted raw values (`"download.requested"`, …) the view already maps
//! to labels and icons.

use std::collections::HashMap;

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::PodcastStore;

/// Hard cap on retained events per episode. Mirrors the Swift
/// `EpisodeAuditLogStore.maxEventsPerEpisode` so behaviour matches across the
/// FFI boundary. Generous — a full download→transcript→chapters run emits well
/// under a dozen events; the cap only bites on repeated manual retries.
const MAX_EVENTS_PER_EPISODE: usize = 200;

/// Severity of a pipeline event. Serialized lowercase to match the Swift
/// `EpisodeAuditEvent.Severity` raw values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventSeverity {
    Info,
    Success,
    Warning,
    Failure,
}

impl EventSeverity {
    fn as_str(self) -> &'static str {
        match self {
            EventSeverity::Info => "info",
            EventSeverity::Success => "success",
            EventSeverity::Warning => "warning",
            EventSeverity::Failure => "failure",
        }
    }
}

/// Dotted `kind` discriminators. Kept identical to the Swift
/// `EpisodeAuditEvent.Kind` constants so the existing view renders kernel
/// events with the right label + icon without a translation table.
pub mod stage {
    // Download lifecycle.
    pub const DOWNLOAD_REQUESTED: &str = "download.requested";
    pub const DOWNLOAD_STARTED: &str = "download.started";
    pub const DOWNLOAD_FINISHED: &str = "download.finished";
    pub const DOWNLOAD_FAILED: &str = "download.failed";
    pub const DOWNLOAD_CANCELLED: &str = "download.cancelled";
    pub const DOWNLOAD_DELETED: &str = "download.deleted";

    // Transcript lifecycle.
    pub const TRANSCRIPT_SKIPPED: &str = "transcript.skipped";
    pub const TRANSCRIPT_ATTEMPT: &str = "transcript.attempt";
    pub const TRANSCRIPT_READY: &str = "transcript.ready";
    pub const TRANSCRIPT_FAILED: &str = "transcript.failed";

    // Identification (chapters + ads), compiled from the transcript.
    pub const CHAPTERS_ATTEMPT: &str = "chapters.attempt";
    pub const CHAPTERS_READY: &str = "chapters.ready";
    pub const CHAPTERS_FAILED: &str = "chapters.failed";
    pub const ADS_READY: &str = "ads.ready";

    // Auto-download policy decisions, including deliberate skips.
    pub const AUTO_DOWNLOAD_QUEUED: &str = "auto_download.queued";
    pub const AUTO_DOWNLOAD_DEFERRED: &str = "auto_download.deferred";
}

/// One structured key/value pair surfaced in the expanded Diagnostics row.
/// Ordered (not a map) so the UI can present URL first, then status, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDetail {
    pub label: String,
    pub value: String,
}

impl EventDetail {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

/// One entry in an episode's pipeline event log. Field names and formats are
/// pinned to the Swift `EpisodeAuditEvent` decoder (see module docs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeEvent {
    pub id: String,
    #[serde(rename = "episodeID")]
    pub episode_id: String,
    /// RFC3339, seconds precision, `Z` suffix — what Swift `.iso8601` expects.
    pub timestamp: String,
    pub kind: String,
    pub severity: String,
    pub summary: String,
    pub details: Vec<EventDetail>,
}

/// Canonicalize an episode id to a single key form so events emitted with the
/// Swift `UUID.uuidString` (uppercase) and the Rust `Uuid` `Display` form
/// (lowercase) — e.g. the auto-download path, which dispatches by
/// `EpisodeId.0.to_string()` — land in the *same* log. The lazy getter is
/// queried with the Swift uppercase form, so both ends must agree. Falls back
/// to a lowercased copy when the id isn't a parseable UUID (defensive — every
/// real episode id is a UUIDv5).
fn canonical_id(episode_id: &str) -> String {
    episode_id
        .parse::<Uuid>()
        .map(|u| u.to_string())
        .unwrap_or_else(|_| episode_id.to_ascii_lowercase())
}

impl PodcastStore {
    /// Record a pipeline event for `episode_id`. Appends to the in-memory log
    /// (hydrating it from disk on first touch this session so prior-session
    /// history survives) and writes the episode's event file. Never touches
    /// `podcasts.json`.
    pub fn emit_event(
        &mut self,
        episode_id: &str,
        kind: &str,
        severity: EventSeverity,
        summary: impl Into<String>,
        details: Vec<EventDetail>,
    ) {
        let key = canonical_id(episode_id);
        self.hydrate_events(&key);
        let event = EpisodeEvent {
            id: Uuid::new_v4().to_string(),
            episode_id: key.clone(),
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            kind: kind.to_owned(),
            severity: severity.as_str().to_owned(),
            summary: summary.into(),
            details,
        };
        let list = self.episode_events.entry(key.clone()).or_default();
        list.push(event);
        if list.len() > MAX_EVENTS_PER_EPISODE {
            let overflow = list.len() - MAX_EVENTS_PER_EPISODE;
            list.drain(0..overflow);
        }
        self.persist_events(&key);
    }

    /// Convenience: record an event with no structured detail rows.
    pub fn emit_event_simple(
        &mut self,
        episode_id: &str,
        kind: &str,
        severity: EventSeverity,
        summary: impl Into<String>,
    ) {
        self.emit_event(episode_id, kind, severity, summary, Vec::new());
    }

    /// All events for `episode_id`, oldest-first (the iOS view sorts for
    /// display). Reads memory when hydrated, otherwise the on-disk file.
    pub fn episode_events(&mut self, episode_id: &str) -> Vec<EpisodeEvent> {
        let key = canonical_id(episode_id);
        self.hydrate_events(&key);
        self.episode_events.get(&key).cloned().unwrap_or_default()
    }

    /// Load `episode_id`'s events from disk into the in-memory map exactly once
    /// per session. Subsequent calls are no-ops (map presence == hydrated), so
    /// an append never clobbers prior-session history.
    fn hydrate_events(&mut self, episode_id: &str) {
        if self.episode_events.contains_key(episode_id) {
            return;
        }
        let loaded = self
            .events_file(episode_id)
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Vec<EpisodeEvent>>(&bytes).ok())
            .unwrap_or_default();
        self.episode_events.insert(episode_id.to_owned(), loaded);
    }

    /// Write `episode_id`'s current event list to its JSON file. Silent no-op
    /// when no data dir is bound (D6) — the in-memory log stays authoritative.
    fn persist_events(&self, episode_id: &str) {
        let Some(path) = self.events_file(episode_id) else {
            return;
        };
        let Some(list) = self.episode_events.get(episode_id) else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec(list) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Path to `episode_id`'s event file, or `None` when no data dir is bound.
    fn events_file(&self, episode_id: &str) -> Option<std::path::PathBuf> {
        self.data_dir().map(|dir| {
            dir.join("episode-events")
                .join(format!("{episode_id}.json"))
        })
    }
}

/// In-memory backing for the per-episode event logs. A field of
/// [`PodcastStore`]; constructed empty and hydrated lazily per episode.
pub type EpisodeEventMap = HashMap<String, Vec<EpisodeEvent>>;

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
