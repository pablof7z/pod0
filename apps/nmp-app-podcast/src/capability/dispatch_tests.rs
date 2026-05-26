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
    store.set_local_path(typed_id, "/var/mobile/seeded.mp3".into());
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
    store.set_local_path(typed_id, "/var/mobile/seeded.mp3".into());

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
    assert!(matches!(outcome, DispatchOutcome::Ok { follow_up_json: None }));
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
    let DispatchOutcome::Ok { follow_up_json } = outcome else {
        panic!("expected ok");
    };
    assert_eq!(
        follow_up_json.as_deref(),
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
