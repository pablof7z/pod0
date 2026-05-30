//! Tests for [`super::triage_state`] — the M4 capability-report side-maps
//! (AI Inbox triage, RAG metadata-indexed coverage, transient transcript
//! status). Covers in-memory set/get/clear semantics, idempotency, the
//! normalization invariants, and a full disk persist → reload round-trip.

use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// RAII tempdir — same lightweight pattern as `persistence_tests` to avoid a
/// `tempfile` dependency.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nmp-podcast-triage-{}-{}",
            std::process::id(),
            n,
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

// ── Triage ──────────────────────────────────────────────────────────────

#[test]
fn triage_for_returns_none_when_unknown() {
    let store = PodcastStore::new();
    assert!(store.triage_for("ep-x").is_none());
}

#[test]
fn set_triage_inbox_round_trips_with_rationale() {
    let mut store = PodcastStore::new();
    let changed = store.set_episode_triage("ep-1", "inbox", true, Some("Because AI".into()));
    assert!(changed);
    let got = store.triage_for("ep-1").expect("present");
    assert_eq!(got.0, "inbox");
    assert!(got.1);
    assert_eq!(got.2.as_deref(), Some("Because AI"));
}

#[test]
fn set_triage_archived_drops_rationale() {
    // Archived episodes never carry a rationale — the store normalizes it
    // away so the two stores (iOS + Rust) can't drift.
    let mut store = PodcastStore::new();
    store.set_episode_triage("ep-1", "archived", false, Some("ignored".into()));
    let got = store.triage_for("ep-1").expect("present");
    assert_eq!(got.0, "archived");
    assert!(!got.1);
    assert_eq!(got.2, None);
}

#[test]
fn set_triage_none_sentinel_clears_entry() {
    let mut store = PodcastStore::new();
    store.set_episode_triage("ep-1", "inbox", false, Some("r".into()));
    let cleared = store.set_episode_triage("ep-1", "none", false, None);
    assert!(cleared);
    assert!(store.triage_for("ep-1").is_none());
}

#[test]
fn set_triage_is_idempotent() {
    let mut store = PodcastStore::new();
    assert!(store.set_episode_triage("ep-1", "inbox", true, Some("r".into())));
    // Identical write reports no change (so the handler skips the rev bump).
    assert!(!store.set_episode_triage("ep-1", "inbox", true, Some("r".into())));
}

#[test]
fn clearing_unknown_episode_reports_no_change() {
    let mut store = PodcastStore::new();
    assert!(!store.set_episode_triage("ep-x", "none", false, None));
}

// ── Metadata-indexed coverage ─────────────────────────────────────────────

#[test]
fn metadata_indexed_defaults_false() {
    let store = PodcastStore::new();
    assert!(!store.is_metadata_indexed("ep-x"));
}

#[test]
fn mark_metadata_indexed_batch_round_trips() {
    let mut store = PodcastStore::new();
    let changed = store.mark_episodes_metadata_indexed(vec!["ep-1", "ep-2"]);
    assert!(changed);
    assert!(store.is_metadata_indexed("ep-1"));
    assert!(store.is_metadata_indexed("ep-2"));
    assert!(!store.is_metadata_indexed("ep-3"));
}

#[test]
fn mark_metadata_indexed_is_idempotent() {
    let mut store = PodcastStore::new();
    assert!(store.mark_episodes_metadata_indexed(vec!["ep-1"]));
    // Re-marking the same id reports no change.
    assert!(!store.mark_episodes_metadata_indexed(vec!["ep-1"]));
    // A batch that mixes a new id with a known one still reports change.
    assert!(store.mark_episodes_metadata_indexed(vec!["ep-1", "ep-2"]));
}

#[test]
fn mark_metadata_indexed_empty_batch_is_noop() {
    let mut store = PodcastStore::new();
    assert!(!store.mark_episodes_metadata_indexed(Vec::<String>::new()));
}

// ── Transcript status override ────────────────────────────────────────────

#[test]
fn transcript_status_returns_none_when_unset() {
    let store = PodcastStore::new();
    assert!(store.transcript_status_for("ep-x").is_none());
}

#[test]
fn set_transcript_status_transcribing_round_trips() {
    let mut store = PodcastStore::new();
    assert!(store.set_transcript_status("ep-1", "transcribing", None));
    let got = store.transcript_status_for("ep-1").expect("present");
    assert_eq!(got.0, "transcribing");
    assert_eq!(got.1, None);
}

#[test]
fn set_transcript_status_failed_keeps_message() {
    let mut store = PodcastStore::new();
    store.set_transcript_status("ep-1", "failed", Some("network down".into()));
    let got = store.transcript_status_for("ep-1").expect("present");
    assert_eq!(got.0, "failed");
    assert_eq!(got.1.as_deref(), Some("network down"));
}

#[test]
fn set_transcript_status_non_failed_drops_message() {
    let mut store = PodcastStore::new();
    store.set_transcript_status("ep-1", "queued", Some("ignored".into()));
    let got = store.transcript_status_for("ep-1").expect("present");
    assert_eq!(got.1, None);
}

#[test]
fn set_transcript_status_none_clears_override() {
    let mut store = PodcastStore::new();
    store.set_transcript_status("ep-1", "transcribing", None);
    assert!(store.set_transcript_status("ep-1", "none", None));
    assert!(store.transcript_status_for("ep-1").is_none());
}

#[test]
fn set_transcript_status_empty_clears_override() {
    let mut store = PodcastStore::new();
    store.set_transcript_status("ep-1", "transcribing", None);
    assert!(store.set_transcript_status("ep-1", "", None));
    assert!(store.transcript_status_for("ep-1").is_none());
}

#[test]
fn set_transcript_status_is_idempotent() {
    let mut store = PodcastStore::new();
    assert!(store.set_transcript_status("ep-1", "transcribing", None));
    assert!(!store.set_transcript_status("ep-1", "transcribing", None));
}

// ── Disk persist → reload round-trip ──────────────────────────────────────

#[test]
fn side_maps_persist_and_reload() {
    let dir = TempDir::new();
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        // Each mutator flushes via persist().
        store.set_episode_triage("ep-1", "inbox", true, Some("listen to this".into()));
        store.set_episode_triage("ep-2", "archived", false, None);
        store.mark_episodes_metadata_indexed(vec!["ep-1", "ep-3"]);
        store.set_transcript_status("ep-1", "failed", Some("boom".into()));
    }
    // Fresh store bound to the same dir hydrates from disk.
    let mut reloaded = PodcastStore::new();
    reloaded.set_data_dir(dir.path.clone());

    let t1 = reloaded.triage_for("ep-1").expect("ep-1 triage present");
    assert_eq!(t1.0, "inbox");
    assert!(t1.1);
    assert_eq!(t1.2.as_deref(), Some("listen to this"));

    let t2 = reloaded.triage_for("ep-2").expect("ep-2 triage present");
    assert_eq!(t2.0, "archived");
    assert_eq!(t2.2, None);

    assert!(reloaded.is_metadata_indexed("ep-1"));
    assert!(reloaded.is_metadata_indexed("ep-3"));
    assert!(!reloaded.is_metadata_indexed("ep-2"));

    let ts = reloaded.transcript_status_for("ep-1").expect("ep-1 status present");
    assert_eq!(ts.0, "failed");
    assert_eq!(ts.1.as_deref(), Some("boom"));
}

#[test]
fn cleared_state_does_not_resurrect_after_reload() {
    // Regression guard for `clearTriageDecision` (iOS) dispatching the "none"
    // sentinel: once cleared, the entry must not come back on reload.
    let dir = TempDir::new();
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.path.clone());
        store.set_episode_triage("ep-1", "inbox", false, Some("r".into()));
        store.set_episode_triage("ep-1", "none", false, None);
    }
    let mut reloaded = PodcastStore::new();
    reloaded.set_data_dir(dir.path.clone());
    assert!(reloaded.triage_for("ep-1").is_none());
}
