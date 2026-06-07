use std::time::{Duration, UNIX_EPOCH};

use super::*;

use chrono::Utc;
use crate::download::{DownloadItemState, DownloadQueue};
use podcast_core::{Episode, Podcast};
use url::Url;

fn t0() -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

/// Build a store with one podcast + one episode and return the episode id
/// as the string the wire format uses.
fn store_with_one_episode() -> (crate::store::PodcastStore, String) {
    let mut store = crate::store::PodcastStore::new();
    let podcast = Podcast::new("Show");
    let ep = Episode::new(
        podcast.id,
        "https://ex.com/feed.xml",
        "guid-1",
        "Title",
        Url::parse("https://ex.com/ep.mp3").expect("url"),
        Utc::now(),
    );
    let id_str = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);
    (store, id_str)
}

#[test]
fn playing_report_json_round_trip_no_follow_up() {
    let mut actor = crate::player::PlayerActor::new();
    let report = r#"{"type":"playing","url":"u","position_secs":1.0,"duration_secs":10.0}"#;
    let outcome = dispatch_audio_report_json(&mut actor, report, t0());
    match outcome {
        DispatchOutcome::Ok { follow_up_json } => assert!(follow_up_json.is_none()),
        DispatchOutcome::DecodeFailed { error } => panic!("decode failed: {error}"),
    }
    assert!(actor.state().is_playing);
    assert_eq!(actor.state().position_secs, 1.0);
}

#[test]
fn sleep_timer_fired_emits_stop_command_json() {
    let mut actor = crate::player::PlayerActor::new();
    actor.arm_sleep_timer(Duration::from_secs(60), t0());
    let outcome =
        dispatch_audio_report_json(&mut actor, r#"{"type":"sleep_timer_fired"}"#, t0());
    match outcome {
        DispatchOutcome::Ok { follow_up_json } => {
            assert_eq!(follow_up_json.as_deref(), Some(r#"{"type":"stop"}"#));
        }
        DispatchOutcome::DecodeFailed { error } => panic!("decode failed: {error}"),
    }
}

#[test]
fn malformed_report_returns_decode_failed() {
    let mut actor = crate::player::PlayerActor::new();
    let outcome = dispatch_audio_report_json(&mut actor, "not-json", t0());
    assert!(matches!(outcome, DispatchOutcome::DecodeFailed { .. }));
    assert!(!actor.state().is_playing);
}

#[test]
fn encode_audio_command_round_trips() {
    let cmd = crate::capability::AudioCommand::seek(99.0);
    let json = encode_audio_command(&cmd).expect("encode");
    let decoded: crate::capability::AudioCommand = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, cmd);
}

// ── DownloadReport dispatch ─────────────────────────────────────────────────

#[test]
fn completed_report_records_local_path() {
    let (mut store, id_str) = store_with_one_episode();
    let report = format!(
        r#"{{"type":"completed","episode_id":"{id_str}","local_path":"/var/mobile/Downloads/{id_str}.mp3"}}"#
    );
    let outcome = dispatch_download_report_json(&mut store, &report);
    match outcome {
        DispatchOutcome::Ok { follow_up_json } => assert!(follow_up_json.is_none()),
        DispatchOutcome::DecodeFailed { error } => panic!("decode failed: {error}"),
    }
    let typed_id = store
        .episode_enclosure_url(&id_str)
        .map(|(id, _)| id)
        .expect("episode present");
    assert_eq!(
        store.local_path_for(&typed_id),
        Some(&*format!("/var/mobile/Downloads/{id_str}.mp3"))
    );
}

#[test]
fn completed_report_caches_real_file_size() {
    // The completion handler stats the finished file on the actor thread so
    // the main-thread snapshot projection reads a cached size instead of
    // statting per tick. Write a real file and assert its byte length lands.
    let (mut store, id_str) = store_with_one_episode();
    let mut path = std::env::temp_dir();
    path.push(format!("fsproj-{id_str}.mp3"));
    let payload = b"podcast-bytes-1234567890"; // 24 bytes
    std::fs::write(&path, payload).expect("write temp file");

    let report = format!(
        r#"{{"type":"completed","episode_id":"{id_str}","local_path":"{}"}}"#,
        path.to_string_lossy()
    );
    let outcome = dispatch_download_report_json(&mut store, &report);
    assert!(matches!(outcome, DispatchOutcome::Ok { .. }));

    let typed_id = store
        .episode_enclosure_url(&id_str)
        .map(|(id, _)| id)
        .expect("episode present");
    assert_eq!(store.file_size_for(&typed_id), Some(payload.len() as i64));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn completed_report_with_missing_file_caches_zero() {
    // An unreadable / already-gone file yields size 0 (treated as unknown by
    // the projection) — the path is still recorded so the episode reads as
    // downloaded.
    let (mut store, id_str) = store_with_one_episode();
    let report = format!(
        r#"{{"type":"completed","episode_id":"{id_str}","local_path":"/nonexistent/{id_str}.mp3"}}"#
    );
    let outcome = dispatch_download_report_json(&mut store, &report);
    assert!(matches!(outcome, DispatchOutcome::Ok { .. }));

    let typed_id = store
        .episode_enclosure_url(&id_str)
        .map(|(id, _)| id)
        .expect("episode present");
    assert!(store.local_path_for(&typed_id).is_some());
    assert_eq!(store.file_size_for(&typed_id), Some(0));
}

#[test]
fn completed_report_for_unknown_episode_is_noop() {
    let (mut store, _) = store_with_one_episode();
    let report =
        r#"{"type":"completed","episode_id":"00000000-0000-0000-0000-000000000000","local_path":"/tmp/x.mp3"}"#;
    let outcome = dispatch_download_report_json(&mut store, report);
    assert!(matches!(outcome, DispatchOutcome::Ok { .. }));
}

#[test]
fn cancelled_report_clears_local_path() {
    let (mut store, id_str) = store_with_one_episode();
    let typed_id = store
        .episode_enclosure_url(&id_str)
        .map(|(id, _)| id)
        .expect("episode present");
    store.set_local_path(typed_id, "/var/mobile/seeded.mp3".into(), 0);
    assert!(store.local_path_for(&typed_id).is_some());

    let report = format!(r#"{{"type":"cancelled","episode_id":"{id_str}"}}"#);
    let outcome = dispatch_download_report_json(&mut store, &report);
    assert!(matches!(outcome, DispatchOutcome::Ok { .. }));
    assert!(store.local_path_for(&typed_id).is_none());
}

#[test]
fn progress_failed_paused_decode_without_mutating_store() {
    let (mut store, id_str) = store_with_one_episode();
    let typed_id = store
        .episode_enclosure_url(&id_str)
        .map(|(id, _)| id)
        .expect("episode present");
    store.set_local_path(typed_id, "/var/mobile/seeded.mp3".into(), 0);

    for report in [
        format!(
            r#"{{"type":"progress","episode_id":"{id_str}","bytes_downloaded":4096,"total_bytes":81920}}"#
        ),
        format!(r#"{{"type":"failed","episode_id":"{id_str}","error":"timeout"}}"#),
        format!(r#"{{"type":"paused","episode_id":"{id_str}","bytes_downloaded":1024}}"#),
    ] {
        let outcome = dispatch_download_report_json(&mut store, &report);
        assert!(
            matches!(outcome, DispatchOutcome::Ok { .. }),
            "{report} should decode cleanly"
        );
    }
    assert_eq!(store.local_path_for(&typed_id), Some("/var/mobile/seeded.mp3"));
}

#[test]
fn malformed_download_report_returns_decode_failed() {
    let (mut store, _) = store_with_one_episode();
    let outcome = dispatch_download_report_json(&mut store, "not-json");
    assert!(matches!(outcome, DispatchOutcome::DecodeFailed { .. }));
}

#[test]
fn progress_report_updates_download_queue() {
    let (mut store, id_str) = store_with_one_episode();
    let mut queue = DownloadQueue::new();
    let _ = queue.enqueue(id_str.clone(), "https://ex.com/ep.mp3");

    let report = format!(
        r#"{{"type":"progress","episode_id":"{id_str}","bytes_downloaded":4096,"total_bytes":8192}}"#
    );
    let outcome = dispatch_download_report_json_with_queue(&mut store, &mut queue, &report);
    // A progress tick changes only transient queue state — it must NOT report a
    // durable change (the FFI uses this to skip the global snapshot `rev` bump).
    assert!(!outcome.decode_failed);
    assert!(!outcome.durable_changed, "progress must not be durable");
    assert!(outcome.follow_up_json.is_none());
    let item = queue.get(&id_str).expect("queued item");
    assert_eq!(item.state, DownloadItemState::Active);
    assert_eq!(item.bytes_downloaded, 4096);
    assert_eq!(item.total_bytes, Some(8192));
}

#[test]
fn completed_report_updates_store_queue_and_returns_next_start() {
    let (mut store, id_str) = store_with_one_episode();
    let mut queue = DownloadQueue::with_capacity(1);
    let _ = queue.enqueue(id_str.clone(), "https://ex.com/ep.mp3");
    assert!(queue.enqueue("ep-2", "https://ex.com/ep-2.mp3").is_none());

    let report = format!(
        r#"{{"type":"completed","episode_id":"{id_str}","local_path":"/var/mobile/Downloads/{id_str}.mp3"}}"#
    );
    let outcome = dispatch_download_report_json_with_queue(&mut store, &mut queue, &report);
    assert!(!outcome.decode_failed);
    // A completed download flips `Episode.downloadState` to `.downloaded` — a
    // durable library change that MUST bump the global snapshot `rev`.
    assert!(outcome.durable_changed, "completion must be durable");
    assert_eq!(
        outcome.follow_up_json.as_deref(),
        Some(r#"{"type":"start_download","url":"https://ex.com/ep-2.mp3","episode_id":"ep-2"}"#)
    );

    let typed_id = store
        .episode_enclosure_url(&id_str)
        .map(|(id, _)| id)
        .expect("episode present");
    assert_eq!(
        store.local_path_for(&typed_id),
        Some(&*format!("/var/mobile/Downloads/{id_str}.mp3"))
    );
    assert_eq!(queue.get(&id_str).unwrap().state, DownloadItemState::Completed);
    assert_eq!(queue.get("ep-2").unwrap().state, DownloadItemState::Active);
}

#[test]
fn completed_report_with_uppercase_episode_id_is_durable() {
    // The iOS shell sends `UUID.uuidString` (UPPERCASE); the store renders the
    // id lowercase. The completion lookup must match case-insensitively, or a
    // finished download silently fails to record `local_path` and never flips
    // to `.downloaded` (durable_changed stays false → no snapshot rev bump).
    let (mut store, id_str) = store_with_one_episode();
    let mut queue = DownloadQueue::new();
    let _ = queue.enqueue(id_str.clone(), "https://ex.com/ep.mp3");

    let upper = id_str.to_uppercase();
    assert_ne!(upper, id_str, "fixture episode id should be lowercase");
    let report = format!(
        r#"{{"type":"completed","episode_id":"{upper}","local_path":"/var/mobile/Downloads/{upper}.mp3"}}"#
    );
    let outcome = dispatch_download_report_json_with_queue(&mut store, &mut queue, &report);
    assert!(!outcome.decode_failed);
    assert!(
        outcome.durable_changed,
        "an UPPERCASE-id completion must resolve the episode and record the path"
    );
    let typed_id = store
        .episode_enclosure_url(&id_str)
        .map(|(id, _)| id)
        .expect("episode present");
    assert_eq!(
        store.local_path_for(&typed_id),
        Some(&*format!("/var/mobile/Downloads/{upper}.mp3"))
    );
}

// ── Pipeline event emission through the real download-report path ────────────

#[test]
fn completed_report_emits_download_finished_event() {
    let (mut store, id_str) = store_with_one_episode();
    let report = format!(
        r#"{{"type":"completed","episode_id":"{id_str}","local_path":"/var/mobile/Downloads/{id_str}.mp3"}}"#
    );
    dispatch_download_report_json(&mut store, &report);
    let events = store.episode_events(&id_str);
    assert!(
        events
            .iter()
            .any(|e| e.kind == crate::store::events::stage::DOWNLOAD_FINISHED),
        "completion must emit download.finished; got {:?}",
        events.iter().map(|e| e.kind.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn failed_report_emits_download_failed_event_with_error_detail() {
    let (mut store, id_str) = store_with_one_episode();
    let report = format!(r#"{{"type":"failed","episode_id":"{id_str}","error":"HTTP 500"}}"#);
    dispatch_download_report_json(&mut store, &report);
    let events = store.episode_events(&id_str);
    let failed = events
        .iter()
        .find(|e| e.kind == crate::store::events::stage::DOWNLOAD_FAILED)
        .expect("failure must emit download.failed");
    assert_eq!(failed.severity, "failure");
    assert!(
        failed.details.iter().any(|d| d.value.contains("HTTP 500")),
        "the failure reason must be captured in the event detail"
    );
}

#[test]
fn cancelled_report_emits_download_cancelled_event() {
    let (mut store, id_str) = store_with_one_episode();
    let report = format!(r#"{{"type":"cancelled","episode_id":"{id_str}"}}"#);
    dispatch_download_report_json(&mut store, &report);
    assert!(
        store
            .episode_events(&id_str)
            .iter()
            .any(|e| e.kind == crate::store::events::stage::DOWNLOAD_CANCELLED),
        "cancellation must be recorded in the diagnostics trail"
    );
}
