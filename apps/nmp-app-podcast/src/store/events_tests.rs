//! Unit tests for the per-episode pipeline event log.

use super::{stage, EventDetail, EventSeverity};
use crate::store::PodcastStore;

const EP: &str = "11111111-2222-3333-4444-555555555555";

#[test]
fn emit_then_read_round_trips() {
    let mut store = PodcastStore::new();
    store.emit_event_simple(EP, stage::DOWNLOAD_REQUESTED, EventSeverity::Info, "queued");
    let events = store.episode_events(EP);
    assert_eq!(events.len(), 1);
    let e = &events[0];
    assert_eq!(e.episode_id, EP);
    assert_eq!(e.kind, stage::DOWNLOAD_REQUESTED);
    assert_eq!(e.severity, "info");
    assert_eq!(e.summary, "queued");
    assert!(e.details.is_empty());
    assert!(!e.id.is_empty());
}

#[test]
fn timestamp_is_iso8601_seconds_with_z() {
    let mut store = PodcastStore::new();
    store.emit_event_simple(EP, stage::DOWNLOAD_STARTED, EventSeverity::Info, "go");
    let ts = store.episode_events(EP)[0].timestamp.clone();
    // Swift's `.iso8601` strategy rejects fractional seconds and requires `Z`.
    assert!(ts.ends_with('Z'), "timestamp must end with Z: {ts}");
    assert!(!ts.contains('.'), "timestamp must not carry fractional seconds: {ts}");
    assert_eq!(ts.len(), 20, "expected YYYY-MM-DDTHH:MM:SSZ: {ts}");
}

#[test]
fn events_preserve_insertion_order_and_details() {
    let mut store = PodcastStore::new();
    store.emit_event_simple(EP, stage::DOWNLOAD_REQUESTED, EventSeverity::Info, "a");
    store.emit_event(
        EP,
        stage::DOWNLOAD_FINISHED,
        EventSeverity::Success,
        "b",
        vec![EventDetail::new("Bytes", "42")],
    );
    let events = store.episode_events(EP);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].summary, "a");
    assert_eq!(events[1].summary, "b");
    assert_eq!(events[1].details[0].label, "Bytes");
    assert_eq!(events[1].details[0].value, "42");
}

#[test]
fn cap_drops_oldest() {
    let mut store = PodcastStore::new();
    for i in 0..(super::MAX_EVENTS_PER_EPISODE + 5) {
        store.emit_event_simple(
            EP,
            stage::DOWNLOAD_STARTED,
            EventSeverity::Info,
            format!("evt-{i}"),
        );
    }
    let events = store.episode_events(EP);
    assert_eq!(events.len(), super::MAX_EVENTS_PER_EPISODE);
    // Oldest five were dropped; the first retained is evt-5.
    assert_eq!(events[0].summary, "evt-5");
}

#[test]
fn uppercase_emit_matches_lowercase_read() {
    // Swift queries with `UUID.uuidString` (uppercase); the auto-download path
    // emits with the Rust lowercase `Uuid` form. Both must hit one log.
    let mut store = PodcastStore::new();
    let upper = EP.to_ascii_uppercase();
    store.emit_event_simple(&upper, stage::AUTO_DOWNLOAD_QUEUED, EventSeverity::Info, "q");
    // Read back with the lowercase form.
    let events = store.episode_events(EP);
    assert_eq!(events.len(), 1, "uppercase emit must be visible to lowercase read");
    // And the stored episodeID is canonical (lowercase) regardless of input.
    assert_eq!(events[0].episode_id, EP);
}

#[test]
fn unknown_episode_reads_empty() {
    let mut store = PodcastStore::new();
    assert!(store.episode_events("no-such-id").is_empty());
}

#[test]
fn persists_per_episode_file_and_survives_reload() {
    let dir = std::env::temp_dir().join(format!("evtest-{}", uuid::Uuid::new_v4()));
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.clone());
        store.emit_event_simple(EP, stage::TRANSCRIPT_READY, EventSeverity::Success, "done");
    }
    // The episode's file exists under episode-events/.
    let file = dir.join("episode-events").join(format!("{EP}.json"));
    assert!(file.exists(), "event file should exist: {file:?}");
    // A fresh store bound to the same dir reads the prior-session history and
    // appends without clobbering it.
    {
        let mut store = PodcastStore::new();
        store.set_data_dir(dir.clone());
        store.emit_event_simple(EP, stage::DOWNLOAD_DELETED, EventSeverity::Info, "later");
        let events = store.episode_events(EP);
        assert_eq!(events.len(), 2, "prior-session event must survive");
        assert_eq!(events[0].summary, "done");
        assert_eq!(events[1].summary, "later");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
