//! Tests for [super::download] — DownloadCommand and DownloadReport serde round-trips.
//!
//! Extracted from `download.rs` to keep that file under the 500-line hard limit.

use super::*;

#[test]
fn download_command_start_serde_roundtrips() {
    let cmd = DownloadCommand::start("https://ex.com/ep.mp3", "ep-7", Some(12345));
    let json = serde_json::to_string(&cmd).expect("encode");
    assert!(json.contains("\"type\":\"start_download\""));
    assert!(json.contains("\"url\":\"https://ex.com/ep.mp3\""));
    assert!(json.contains("\"episode_id\":\"ep-7\""));
    assert!(json.contains("\"expected_bytes\":12345"));
    let decoded: DownloadCommand = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, cmd);
}

#[test]
fn download_command_start_omits_none_expected_bytes() {
    let cmd = DownloadCommand::start("https://ex.com/ep.mp3", "ep-7", None);
    let json = serde_json::to_string(&cmd).expect("encode");
    // `skip_serializing_if = "Option::is_none"` keeps the wire payload tidy.
    assert!(!json.contains("expected_bytes"));
    let decoded: DownloadCommand = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, cmd);
}

#[test]
fn download_command_cancel_all_has_no_payload() {
    assert_eq!(
        serde_json::to_string(&DownloadCommand::CancelAll).expect("encode"),
        r#"{"type":"cancel_all"}"#
    );
}

#[test]
fn download_command_pause_resume_cancel_round_trip() {
    for cmd in [
        DownloadCommand::PauseDownload {
            episode_id: "ep-1".into(),
        },
        DownloadCommand::ResumeDownload {
            episode_id: "ep-1".into(),
        },
        DownloadCommand::CancelDownload {
            episode_id: "ep-1".into(),
        },
    ] {
        let json = serde_json::to_string(&cmd).expect("encode");
        let decoded: DownloadCommand = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, cmd);
    }
}

#[test]
fn download_report_progress_serde_roundtrips() {
    let rep = DownloadReport::Progress {
        episode_id: "ep-1".into(),
        bytes_downloaded: 4096,
        total_bytes: Some(81920),
    };
    let json = serde_json::to_string(&rep).expect("encode");
    let decoded: DownloadReport = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, rep);
}

#[test]
fn download_report_progress_total_bytes_optional() {
    let rep = DownloadReport::Progress {
        episode_id: "ep-1".into(),
        bytes_downloaded: 4096,
        total_bytes: None,
    };
    let json = serde_json::to_string(&rep).expect("encode");
    assert!(!json.contains("total_bytes"));
    let decoded: DownloadReport = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, rep);
}

#[test]
fn download_report_completed_carries_local_path() {
    let rep = DownloadReport::Completed {
        episode_id: "ep-1".into(),
        local_path: "/var/mobile/.../ep-1.mp3".into(),
    };
    let json = serde_json::to_string(&rep).expect("encode");
    assert!(json.contains("\"type\":\"completed\""));
    assert!(json.contains("ep-1.mp3"));
    let decoded: DownloadReport = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, rep);
}

#[test]
fn download_report_failed_carries_error() {
    let rep = DownloadReport::Failed {
        episode_id: "ep-1".into(),
        error: "transport: timeout".into(),
    };
    let json = serde_json::to_string(&rep).expect("encode");
    assert!(json.contains("\"type\":\"failed\""));
    assert!(json.contains("transport: timeout"));
}

#[test]
fn download_report_paused_carries_bytes() {
    let rep = DownloadReport::Paused {
        episode_id: "ep-1".into(),
        bytes_downloaded: 2048,
    };
    let json = serde_json::to_string(&rep).expect("encode");
    let decoded: DownloadReport = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, rep);
}

#[test]
fn namespace_matches_canonical_capability_plan() {
    assert_eq!(DOWNLOAD_CAPABILITY_NAMESPACE, "nmp.download.capability");
}
