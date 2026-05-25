//! `handle_report` projection and slot-frees-up follow-up logic.

use super::*;
use crate::capability::DownloadReport;

/// Helper: enqueue 5 items with cap=3, returning the active+queued split.
fn fill_to_five_with_cap_three() -> DownloadQueue {
    let mut q = DownloadQueue::with_capacity(3);
    for i in 1..=5 {
        let _ = q.enqueue(format!("ep-{i}"), format!("https://ex.com/{i}.mp3"));
    }
    assert_eq!(q.active_count(), 3);
    assert_eq!(q.queued_count(), 2);
    q
}

#[test]
fn completed_frees_slot_and_starts_next_queued() {
    let mut q = fill_to_five_with_cap_three();
    let follow_ups = q.handle_report(DownloadReport::Completed {
        episode_id: "ep-1".into(),
        local_path: "/tmp/ep-1.mp3".into(),
    });
    assert_eq!(follow_ups.len(), 1);
    assert_eq!(start_id(&follow_ups[0]), Some("ep-4"));

    assert_eq!(
        q.get("ep-1").map(|i| i.state),
        Some(DownloadItemState::Completed)
    );
    assert_eq!(q.get("ep-1").and_then(|i| i.local_path.as_deref()),
               Some("/tmp/ep-1.mp3"));
    assert_eq!(
        q.get("ep-4").map(|i| i.state),
        Some(DownloadItemState::Active)
    );
    assert_eq!(q.active_count(), 3);
    assert_eq!(q.queued_count(), 1);
}

#[test]
fn failed_frees_slot_and_starts_next_queued() {
    let mut q = fill_to_five_with_cap_three();
    let follow_ups = q.handle_report(DownloadReport::Failed {
        episode_id: "ep-2".into(),
        error: "transport: timeout".into(),
    });
    assert_eq!(follow_ups.len(), 1);
    assert_eq!(start_id(&follow_ups[0]), Some("ep-4"));

    assert_eq!(
        q.get("ep-2").map(|i| i.state),
        Some(DownloadItemState::Failed)
    );
    assert_eq!(
        q.get("ep-2").and_then(|i| i.error.as_deref()),
        Some("transport: timeout")
    );
    assert_eq!(q.active_count(), 3);
    assert_eq!(q.queued_count(), 1);
}

#[test]
fn cancelled_report_frees_slot_and_starts_next_queued() {
    let mut q = fill_to_five_with_cap_three();
    let follow_ups = q.handle_report(DownloadReport::Cancelled {
        episode_id: "ep-1".into(),
    });
    assert_eq!(follow_ups.len(), 1);
    assert_eq!(start_id(&follow_ups[0]), Some("ep-4"));

    assert_eq!(
        q.get("ep-1").map(|i| i.state),
        Some(DownloadItemState::Cancelled)
    );
    assert_eq!(q.active_count(), 3);
}

#[test]
fn progress_updates_bytes_without_starting_anything() {
    let mut q = fill_to_five_with_cap_three();
    let follow_ups = q.handle_report(DownloadReport::Progress {
        episode_id: "ep-1".into(),
        bytes_downloaded: 4096,
        total_bytes: Some(8192),
    });
    assert!(follow_ups.is_empty());
    let item = q.get("ep-1").expect("present");
    assert_eq!(item.bytes_downloaded, 4096);
    assert_eq!(item.total_bytes, Some(8192));
    assert_eq!(item.state, DownloadItemState::Active);
}

#[test]
fn progress_total_bytes_sticky_once_known() {
    let mut q = DownloadQueue::with_capacity(3);
    let _ = q.enqueue("ep-1", "https://ex.com/1.mp3");
    let _ = q.handle_report(DownloadReport::Progress {
        episode_id: "ep-1".into(),
        bytes_downloaded: 1024,
        total_bytes: Some(8192),
    });
    // A later Progress without total_bytes shouldn't clear the known total.
    let _ = q.handle_report(DownloadReport::Progress {
        episode_id: "ep-1".into(),
        bytes_downloaded: 2048,
        total_bytes: None,
    });
    let item = q.get("ep-1").expect("present");
    assert_eq!(item.bytes_downloaded, 2048);
    assert_eq!(item.total_bytes, Some(8192));
}

#[test]
fn completed_without_known_total_falls_back_to_downloaded_bytes() {
    let mut q = DownloadQueue::with_capacity(3);
    let _ = q.enqueue("ep-1", "https://ex.com/1.mp3");
    let _ = q.handle_report(DownloadReport::Progress {
        episode_id: "ep-1".into(),
        bytes_downloaded: 4096,
        total_bytes: None,
    });
    let _ = q.handle_report(DownloadReport::Completed {
        episode_id: "ep-1".into(),
        local_path: "/tmp/ep-1.mp3".into(),
    });
    let item = q.get("ep-1").expect("present");
    assert_eq!(item.total_bytes, Some(4096));
    assert!((item.progress_fraction() - 1.0).abs() < 1e-6);
}

#[test]
fn paused_report_holds_slot_no_follow_up() {
    let mut q = fill_to_five_with_cap_three();
    let follow_ups = q.handle_report(DownloadReport::Paused {
        episode_id: "ep-1".into(),
        bytes_downloaded: 2048,
    });
    assert!(follow_ups.is_empty(), "Paused holds the slot");
    let item = q.get("ep-1").expect("present");
    assert_eq!(item.state, DownloadItemState::Paused);
    assert_eq!(item.bytes_downloaded, 2048);
    // Active count unchanged: Paused still counts as a slot.
    assert_eq!(q.active_count(), 3);
    // Queued count unchanged.
    assert_eq!(q.queued_count(), 2);
}

#[test]
fn progress_on_paused_item_reconciles_back_to_active() {
    let mut q = DownloadQueue::with_capacity(3);
    let _ = q.enqueue("ep-1", "https://ex.com/1.mp3");
    let _ = q.handle_report(DownloadReport::Paused {
        episode_id: "ep-1".into(),
        bytes_downloaded: 2048,
    });
    assert_eq!(
        q.get("ep-1").map(|i| i.state),
        Some(DownloadItemState::Paused)
    );
    let _ = q.handle_report(DownloadReport::Progress {
        episode_id: "ep-1".into(),
        bytes_downloaded: 3072,
        total_bytes: None,
    });
    assert_eq!(
        q.get("ep-1").map(|i| i.state),
        Some(DownloadItemState::Active)
    );
}

#[test]
fn report_for_unknown_episode_is_silent_no_op() {
    let mut q = DownloadQueue::with_capacity(3);
    let follow_ups = q.handle_report(DownloadReport::Completed {
        episode_id: "ghost".into(),
        local_path: "/tmp/ghost.mp3".into(),
    });
    assert!(follow_ups.is_empty());
    assert!(q.get("ghost").is_none());
}

#[test]
fn fifo_order_preserved_across_multiple_completions() {
    let mut q = DownloadQueue::with_capacity(2);
    let _ = q.enqueue("ep-1", "https://ex.com/1.mp3");
    let _ = q.enqueue("ep-2", "https://ex.com/2.mp3");
    let _ = q.enqueue("ep-3", "https://ex.com/3.mp3");
    let _ = q.enqueue("ep-4", "https://ex.com/4.mp3");
    assert_eq!(q.queued_count(), 2);

    let next = q.handle_report(DownloadReport::Completed {
        episode_id: "ep-1".into(),
        local_path: "/tmp/1.mp3".into(),
    });
    assert_eq!(start_id(&next[0]), Some("ep-3"));

    let next = q.handle_report(DownloadReport::Completed {
        episode_id: "ep-2".into(),
        local_path: "/tmp/2.mp3".into(),
    });
    assert_eq!(start_id(&next[0]), Some("ep-4"));

    // Queue drained.
    let next = q.handle_report(DownloadReport::Completed {
        episode_id: "ep-3".into(),
        local_path: "/tmp/3.mp3".into(),
    });
    assert!(next.is_empty());
}
