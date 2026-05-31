use super::*;
use crate::capability::DownloadReport;

#[test]
fn empty_queue_omits_downloads_snapshot() {
    let queue = DownloadQueue::new();
    assert!(build_downloads_snapshot(&queue).is_none());
}

#[test]
fn active_queued_paused_failed_rows_project_in_stable_order() {
    let mut queue = DownloadQueue::with_capacity(3);
    let _ = queue.enqueue("active", "https://ex.com/active.mp3");
    let _ = queue.enqueue("paused", "https://ex.com/paused.mp3");
    let _ = queue.enqueue("failed", "https://ex.com/failed.mp3");
    let _ = queue.handle_report(DownloadReport::Paused {
        episode_id: "paused".into(),
        bytes_downloaded: 5,
    });
    let _ = queue.handle_report(DownloadReport::Progress {
        episode_id: "active".into(),
        bytes_downloaded: 50,
        total_bytes: Some(100),
    });
    let _ = queue.handle_report(DownloadReport::Failed {
        episode_id: "failed".into(),
        error: "timeout".into(),
    });
    queue.max_concurrent = 2;
    assert!(queue
        .enqueue("queued", "https://ex.com/queued.mp3")
        .is_none());

    let snapshot = build_downloads_snapshot(&queue).expect("snapshot");
    let states: Vec<_> = snapshot
        .active
        .iter()
        .map(|item| item.state.as_str())
        .collect();
    assert_eq!(states, ["active", "paused", "queued", "failed"]);
    assert_eq!(snapshot.queued_count, 1);
    assert_eq!(snapshot.active[0].progress, 0.5);
    assert_eq!(snapshot.active[3].error.as_deref(), Some("timeout"));
}

#[test]
fn rows_carry_enclosure_url_for_pull_model_executor() {
    // The Android download capability has no inbound command seam, so it
    // seeds HTTP fetches from the projected `url` on each active/queued
    // row. Guard that the projection actually carries it.
    let mut queue = DownloadQueue::with_capacity(1);
    let _ = queue.enqueue("ep-1", "https://ex.com/ep-1.mp3");
    let snapshot = build_downloads_snapshot(&queue).expect("snapshot");
    assert_eq!(snapshot.active[0].url, "https://ex.com/ep-1.mp3");
}

#[test]
fn completed_and_cancelled_rows_drop_out() {
    let mut queue = DownloadQueue::with_capacity(1);
    let _ = queue.enqueue("done", "https://ex.com/done.mp3");
    let _ = queue.handle_report(DownloadReport::Completed {
        episode_id: "done".into(),
        local_path: "/tmp/done.mp3".into(),
    });
    assert!(build_downloads_snapshot(&queue).is_none());

    let _ = queue.enqueue("cancelled", "https://ex.com/cancelled.mp3");
    let _ = queue.handle_report(DownloadReport::Cancelled {
        episode_id: "cancelled".into(),
    });
    assert!(build_downloads_snapshot(&queue).is_none());
}
