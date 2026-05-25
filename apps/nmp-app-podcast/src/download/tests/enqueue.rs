//! Enqueue + concurrency-cap behaviour.

use super::*;

#[test]
fn empty_queue_starts_first_item_immediately() {
    let mut q = DownloadQueue::with_capacity(3);
    let cmd = q.enqueue("ep-1", "https://ex.com/1.mp3");
    assert!(matches!(cmd, Some(DownloadCommand::StartDownload { .. })));
    assert_eq!(q.active_count(), 1);
    assert_eq!(q.queued_count(), 0);
    assert_eq!(
        q.get("ep-1").map(|i| i.state),
        Some(DownloadItemState::Active)
    );
}

#[test]
fn five_items_with_cap_three_starts_three_immediately() {
    let mut q = DownloadQueue::with_capacity(3);
    let cmd1 = q.enqueue("ep-1", "https://ex.com/1.mp3");
    let cmd2 = q.enqueue("ep-2", "https://ex.com/2.mp3");
    let cmd3 = q.enqueue("ep-3", "https://ex.com/3.mp3");
    let cmd4 = q.enqueue("ep-4", "https://ex.com/4.mp3");
    let cmd5 = q.enqueue("ep-5", "https://ex.com/5.mp3");

    assert!(matches!(cmd1, Some(DownloadCommand::StartDownload { .. })));
    assert!(matches!(cmd2, Some(DownloadCommand::StartDownload { .. })));
    assert!(matches!(cmd3, Some(DownloadCommand::StartDownload { .. })));
    assert!(cmd4.is_none(), "fourth enqueue should queue, not start");
    assert!(cmd5.is_none(), "fifth enqueue should queue, not start");

    assert_eq!(q.active_count(), 3);
    assert_eq!(q.queued_count(), 2);
    assert_eq!(
        q.get("ep-4").map(|i| i.state),
        Some(DownloadItemState::Queued)
    );
    assert_eq!(
        q.get("ep-5").map(|i| i.state),
        Some(DownloadItemState::Queued)
    );
}

#[test]
fn enqueue_is_idempotent_for_active_items() {
    let mut q = DownloadQueue::with_capacity(3);
    let _ = q.enqueue("ep-1", "https://ex.com/1.mp3");
    // Re-issuing for the same id is a no-op.
    let cmd = q.enqueue("ep-1", "https://ex.com/1.mp3");
    assert!(cmd.is_none(), "re-enqueue of active item should be no-op");
    assert_eq!(q.active_count(), 1);
}

#[test]
fn enqueue_after_failed_starts_fresh() {
    let mut q = DownloadQueue::with_capacity(3);
    let _ = q.enqueue("ep-1", "https://ex.com/1.mp3");
    let _ = q.handle_report(crate::capability::DownloadReport::Failed {
        episode_id: "ep-1".into(),
        error: "boom".into(),
    });
    assert_eq!(
        q.get("ep-1").map(|i| i.state),
        Some(DownloadItemState::Failed)
    );
    // Re-enqueue should start a new attempt.
    let cmd = q.enqueue("ep-1", "https://ex.com/1.mp3");
    assert!(matches!(cmd, Some(DownloadCommand::StartDownload { .. })));
    assert_eq!(
        q.get("ep-1").map(|i| i.state),
        Some(DownloadItemState::Active)
    );
}

#[test]
fn cap_of_zero_queues_everything() {
    let mut q = DownloadQueue::with_capacity(0);
    let cmd = q.enqueue("ep-1", "https://ex.com/1.mp3");
    assert!(cmd.is_none());
    assert_eq!(q.queued_count(), 1);
    assert_eq!(q.active_count(), 0);
}

#[test]
fn start_command_carries_url_and_episode_id() {
    let mut q = DownloadQueue::with_capacity(3);
    let cmd = q
        .enqueue("ep-7", "https://ex.com/7.mp3")
        .expect("enqueue starts");
    match cmd {
        DownloadCommand::StartDownload {
            url,
            episode_id,
            expected_bytes,
        } => {
            assert_eq!(url, "https://ex.com/7.mp3");
            assert_eq!(episode_id, "ep-7");
            assert!(expected_bytes.is_none());
        }
        _ => panic!("expected StartDownload"),
    }
}
