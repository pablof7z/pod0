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
