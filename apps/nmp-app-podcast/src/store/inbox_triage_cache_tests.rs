//! Tests for inbox-triage-cache JSON persistence.

use super::*;
use crate::inbox_llm::{TriageResult, TriageStatus};

/// Build a unique temp dir under the OS temp root for a hermetic round-trip.
fn temp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("inbox-triage-cache-{tag}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Cold-load: save a populated cache, reload it from disk, and verify every
/// entry survives byte-for-byte (score, reason, categories, status,
/// attempted_at) for both `Ready` and `Pending` variants.
#[test]
fn save_then_reload_preserves_entries() {
    let dir = temp_dir("roundtrip");

    let mut cache: std::collections::HashMap<String, TriageResult> =
        std::collections::HashMap::new();
    cache.insert(
        "ep-ready".to_string(),
        TriageResult::ready(
            0.82,
            "Deep dive on distributed systems".to_string(),
            vec!["tech".to_string(), "engineering".to_string()],
            1_700_000_000,
        ),
    );
    cache.insert(
        "ep-pending".to_string(),
        TriageResult::pending(1_700_000_500),
    );

    save_triage_cache(&dir, &cache).expect("save should succeed");

    let reloaded = load_triage_cache(&dir);
    assert_eq!(reloaded.len(), 2, "both entries must survive the round-trip");

    let ready = reloaded.get("ep-ready").expect("ready entry present");
    assert_eq!(ready.status, TriageStatus::Ready);
    assert!((ready.priority_score - 0.82).abs() < f32::EPSILON);
    assert_eq!(ready.priority_reason, "Deep dive on distributed systems");
    assert_eq!(ready.categories, vec!["tech", "engineering"]);
    assert_eq!(ready.attempted_at, 1_700_000_000);

    let pending = reloaded.get("ep-pending").expect("pending entry present");
    assert_eq!(pending.status, TriageStatus::Pending);
    assert_eq!(pending.attempted_at, 1_700_000_500);

    let _ = std::fs::remove_dir_all(&dir);
}

/// A missing file is a fresh start, not an error: load yields an empty map.
#[test]
fn load_missing_file_is_empty_not_error() {
    let dir = temp_dir("missing");
    // Intentionally never call save — the cache file does not exist.
    let loaded = load_triage_cache(&dir);
    assert!(loaded.is_empty(), "missing file must load as empty map");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A corrupt cache file degrades to an empty map rather than panicking.
#[test]
fn load_corrupt_file_is_empty() {
    let dir = temp_dir("corrupt");
    std::fs::write(dir.join(INBOX_TRIAGE_CACHE_FILE), b"{ not valid json ").unwrap();
    let loaded = load_triage_cache(&dir);
    assert!(loaded.is_empty(), "corrupt file must load as empty map");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Save leaves no `.tmp` turd behind (atomic rename completed).
#[test]
fn save_is_atomic_no_tmp_left() {
    let dir = temp_dir("atomic");
    let cache = std::collections::HashMap::new();
    save_triage_cache(&dir, &cache).expect("save empty cache");
    let tmp = dir.join(format!("{INBOX_TRIAGE_CACHE_FILE}.tmp"));
    assert!(!tmp.exists(), "tmp file must be renamed away");
    assert!(dir.join(INBOX_TRIAGE_CACHE_FILE).exists(), "final file present");
    let _ = std::fs::remove_dir_all(&dir);
}
