//! Pause / resume / cancel / cancel-all semantics.

use super::*;
use crate::capability::DownloadReport;

#[test]
fn cancel_active_emits_cancel_command_state_pending_report() {
    let mut q = DownloadQueue::with_capacity(3);
    let _ = q.enqueue("ep-1", "https://ex.com/1.mp3");
    let cmd = q.cancel("ep-1");
    match cmd {
        Some(DownloadCommand::CancelDownload { episode_id }) => {
            assert_eq!(episode_id, "ep-1");
        }
        _ => panic!("expected CancelDownload"),
    }
    // State doesn't move to Cancelled until the executor reports it.
    assert_eq!(
        q.get("ep-1").map(|i| i.state),
        Some(DownloadItemState::Active)
    );
}

#[test]
fn cancel_active_starts_next_queued_after_cancelled_report() {
    let mut q = DownloadQueue::with_capacity(3);
    for i in 1..=4 {
        let _ = q.enqueue(format!("ep-{i}"), format!("https://ex.com/{i}.mp3"));
    }
    let _ = q.cancel("ep-1");
    let follow_ups = q.handle_report(DownloadReport::Cancelled {
        episode_id: "ep-1".into(),
    });
    assert_eq!(follow_ups.len(), 1);
    assert_eq!(start_id(&follow_ups[0]), Some("ep-4"));
}

#[test]
fn cancel_queued_marks_cancelled_synchronously_no_command() {
    let mut q = DownloadQueue::with_capacity(3);
    for i in 1..=5 {
        let _ = q.enqueue(format!("ep-{i}"), format!("https://ex.com/{i}.mp3"));
    }
    assert_eq!(
        q.get("ep-4").map(|i| i.state),
        Some(DownloadItemState::Queued)
    );
    let cmd = q.cancel("ep-4");
    assert!(cmd.is_none(), "queued cancel needs no command");
    assert_eq!(
        q.get("ep-4").map(|i| i.state),
        Some(DownloadItemState::Cancelled)
    );
    // ep-5 stays queued (slot wasn't freed — ep-4 didn't hold one).
    assert_eq!(
        q.get("ep-5").map(|i| i.state),
        Some(DownloadItemState::Queued)
    );
}

#[test]
fn cancel_unknown_episode_is_silent_no_op() {
    let mut q = DownloadQueue::with_capacity(3);
    let cmd = q.cancel("ghost");
    assert!(cmd.is_none());
}

#[test]
fn cancel_already_terminal_is_no_op() {
    let mut q = DownloadQueue::with_capacity(3);
    let _ = q.enqueue("ep-1", "https://ex.com/1.mp3");
    let _ = q.handle_report(DownloadReport::Completed {
        episode_id: "ep-1".into(),
        local_path: "/tmp/1.mp3".into(),
    });
    let cmd = q.cancel("ep-1");
    assert!(cmd.is_none());
    assert_eq!(
        q.get("ep-1").map(|i| i.state),
        Some(DownloadItemState::Completed)
    );
}

#[test]
fn pause_active_emits_pause_command() {
    let mut q = DownloadQueue::with_capacity(3);
    let _ = q.enqueue("ep-1", "https://ex.com/1.mp3");
    let cmd = q.pause("ep-1");
    match cmd {
        Some(DownloadCommand::PauseDownload { episode_id }) => {
            assert_eq!(episode_id, "ep-1");
        }
        _ => panic!("expected PauseDownload"),
    }
}

#[test]
fn pause_queued_is_noop() {
    let mut q = DownloadQueue::with_capacity(1);
    let _ = q.enqueue("ep-1", "https://ex.com/1.mp3"); // Active.
    let _ = q.enqueue("ep-2", "https://ex.com/2.mp3"); // Queued.
    let cmd = q.pause("ep-2");
    assert!(cmd.is_none());
}

#[test]
fn resume_paused_emits_resume_command() {
    let mut q = DownloadQueue::with_capacity(3);
    let _ = q.enqueue("ep-1", "https://ex.com/1.mp3");
    let _ = q.handle_report(DownloadReport::Paused {
        episode_id: "ep-1".into(),
        bytes_downloaded: 1024,
    });
    let cmd = q.resume("ep-1");
    match cmd {
        Some(DownloadCommand::ResumeDownload { episode_id }) => {
            assert_eq!(episode_id, "ep-1");
        }
        _ => panic!("expected ResumeDownload"),
    }
}

#[test]
fn resume_active_is_noop() {
    let mut q = DownloadQueue::with_capacity(3);
    let _ = q.enqueue("ep-1", "https://ex.com/1.mp3");
    let cmd = q.resume("ep-1");
    assert!(cmd.is_none());
}

#[test]
fn cancel_all_with_active_items_emits_cancel_all() {
    let mut q = DownloadQueue::with_capacity(2);
    let _ = q.enqueue("ep-1", "https://ex.com/1.mp3");
    let _ = q.enqueue("ep-2", "https://ex.com/2.mp3");
    let _ = q.enqueue("ep-3", "https://ex.com/3.mp3"); // queued.
    let cmd = q.cancel_all();
    assert_eq!(cmd, Some(DownloadCommand::CancelAll));
    // Queued items move to Cancelled synchronously.
    assert_eq!(
        q.get("ep-3").map(|i| i.state),
        Some(DownloadItemState::Cancelled)
    );
    // Queue order drained.
    assert_eq!(q.queued_count(), 0);
}

#[test]
fn cancel_all_empty_queue_is_noop() {
    let mut q = DownloadQueue::with_capacity(3);
    assert!(q.cancel_all().is_none());
}

#[test]
fn cancel_all_only_queued_items_no_command_but_clears_queue() {
    let mut q = DownloadQueue::with_capacity(0);
    let _ = q.enqueue("ep-1", "https://ex.com/1.mp3");
    let _ = q.enqueue("ep-2", "https://ex.com/2.mp3");
    assert_eq!(q.queued_count(), 2);
    let cmd = q.cancel_all();
    assert!(cmd.is_none(), "no executor work needed when nothing active");
    assert_eq!(q.queued_count(), 0);
    assert_eq!(
        q.get("ep-1").map(|i| i.state),
        Some(DownloadItemState::Cancelled)
    );
}
