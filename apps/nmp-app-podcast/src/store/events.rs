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

    /// Parse a wire severity string (as sent by the Swift generic
    /// record-event FFI). Unknown / absent values fall back to `Info` so a
    /// malformed severity never drops the event.
    pub fn from_wire(raw: &str) -> Self {
        match raw {
            "success" => EventSeverity::Success,
            "warning" => EventSeverity::Warning,
            "failure" => EventSeverity::Failure,
            _ => EventSeverity::Info,
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
    /// Transcript chunks embedded + upserted into the RAG vector index. The
    /// transcript is readable before this; indexing is a separate best-effort
    /// stage (needs an embeddings key) whose outcome the user can't otherwise
    /// see. Both the success and the failure deserve a line in the log.
    pub const TRANSCRIPT_INDEXED: &str = "transcript.indexed";
    pub const TRANSCRIPT_INDEX_FAILED: &str = "transcript.index.failed";

    // Identification (chapters + ads), compiled from the transcript.
    pub const CHAPTERS_ATTEMPT: &str = "chapters.attempt";
    pub const CHAPTERS_READY: &str = "chapters.ready";
    pub const CHAPTERS_FAILED: &str = "chapters.failed";
    pub const ADS_READY: &str = "ads.ready";

    // Playback lifecycle. Emitted at the authoritative kernel seams (play
    // dispatch, mark-played) so Diagnostics shows when the user actually
    // listened and from where, not just the pipeline's processing of the file.
    pub const PLAYBACK_STARTED: &str = "playback.started";
    pub const PLAYBACK_COMPLETED: &str = "playback.completed";

    // Clipping lifecycle. The clip composer (Swift) records each stage through
    // the generic record-event FFI so a failed export is visible here instead
    // of vanishing into a logger line.
    pub const CLIP_CREATED: &str = "clip.created";
    pub const CLIP_EXPORTED: &str = "clip.exported";
    pub const CLIP_SHARED: &str = "clip.shared";
    pub const CLIP_FAILED: &str = "clip.failed";

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

    /// Record a [`stage::TRANSCRIPT_SKIPPED`] event explaining why the iOS
    /// pipeline declined to transcribe `episode_id` (per-category opt-out,
    /// automatic AI transcription off, missing provider key, on-device audio
    /// not on disk). `reason` rides a single `Reason` detail row; `None`
    /// records a bare skip.
    ///
    /// Idempotent: a no-op when the episode's most recent event is already an
    /// identical skip (same reason). `ingest()` re-runs on repeatable
    /// speculative paths (episode-detail appear, library warmup), so without
    /// this a muted episode would pile duplicate rows onto its capped log.
    pub fn record_transcript_skip(&mut self, episode_id: &str, reason: Option<String>) {
        let already_recorded = self.episode_events(episode_id).last().map_or(false, |e| {
            e.kind == stage::TRANSCRIPT_SKIPPED
                && e.details.first().map(|d| d.value.as_str()) == reason.as_deref()
        });
        if already_recorded {
            return;
        }
        self.emit_event(
            episode_id,
            stage::TRANSCRIPT_SKIPPED,
            EventSeverity::Info,
            "Transcription skipped",
            reason
                .map(|r| vec![EventDetail::new("Reason", r)])
                .unwrap_or_default(),
        );
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
