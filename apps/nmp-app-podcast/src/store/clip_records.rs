//! JSON persistence for Rust-owned clip records.
//!
//! Clips are owned by `state.clips`, not by the library's `podcasts.json`.
//! This sidecar stores the internal `ClipRecord` rows under the app data dir
//! so user-created clips survive restart while remaining a single Rust-owned
//! source of truth.

use std::path::Path;

use crate::clip_handler::ClipRecord;

pub const CLIP_RECORDS_FILE: &str = "clips.json";

/// Write all clip records to `<data_dir>/clips.json`.
pub fn save_clip_records(dir: &Path, clips: &[ClipRecord]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_vec_pretty(clips).map_err(|e| e.to_string())?;
    let final_path = dir.join(CLIP_RECORDS_FILE);
    let tmp_path = dir.join(format!("{CLIP_RECORDS_FILE}.tmp"));
    std::fs::write(&tmp_path, &json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_path, &final_path).map_err(|e| e.to_string())
}

/// Load `<data_dir>/clips.json`.
///
/// `None` means no valid sidecar exists. `Some(vec![])` is a valid persisted
/// empty clip list.
#[must_use]
pub fn load_clip_records(dir: &Path) -> Option<Vec<ClipRecord>> {
    let path = dir.join(CLIP_RECORDS_FILE);
    let bytes = std::fs::read(&path).ok()?;
    serde_json::from_slice::<Vec<ClipRecord>>(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip_handler::ClipRecord;

    #[test]
    fn test_orphan_seed_json_parses() {
        let json = r#"[{
            "id": "05480548-0548-0548-0548-054800000001",
            "episode_id": "deadbeef-dead-dead-dead-000000000001",
            "episode_title": "Orphaned Episode",
            "podcast_title": "This American Life",
            "start_secs": 60.0,
            "end_secs": 90.0,
            "title": "Orphan clip",
            "transcript_text": "economy is not going",
            "speaker": null,
            "source": "touch",
            "refinement_status": "manual",
            "auto_snip_anchor_secs": null,
            "created_at": 1779212388
        }]"#;
        let clips: Vec<ClipRecord> = serde_json::from_str(json).expect("should parse seed JSON");
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].title, Some("Orphan clip".to_owned()));
        assert_eq!(clips[0].transcript_text, "economy is not going");
        assert_eq!(clips[0].speaker, None);
        assert_eq!(clips[0].auto_snip_anchor_secs, None);
    }
}
