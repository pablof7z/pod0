//! User notes store — holds local notes created by the user.
//!
//! Notes can be attached to episodes or podcasts. Each note carries
//! metadata for display and synchronization.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// A single user note with metadata.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct UserNote {
    pub id: String,
    pub text: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<NoteTarget>,
    pub created_at: i64,
    pub deleted: bool,
    pub author: String,
}

/// The target of a note — either an episode or a podcast.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum NoteTarget {
    #[serde(rename = "episode")]
    Episode {
        episode_id: String,
        position_secs: f64,
    },
    #[serde(rename = "podcast")]
    Podcast { podcast_id: String },
}

/// Load notes from disk at the designated data directory.
/// Returns an empty vector if the file doesn't exist or is malformed.
pub fn load_notes(data_dir: &Path) -> Vec<UserNote> {
    let path = data_dir.join("notes.json");
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Atomically write notes to disk using write-to-temp-then-rename.
/// No-op if the write fails silently (per D6 degradation).
pub fn save_notes(data_dir: &Path, notes: &[UserNote]) {
    let json = match serde_json::to_string(notes) {
        Ok(j) => j,
        Err(_) => return,
    };
    let tmp = data_dir.join("notes.json.tmp");
    let dest = data_dir.join("notes.json");
    if std::fs::write(&tmp, &json).is_ok() {
        let _ = std::fs::rename(&tmp, &dest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn save_and_load_empty_notes() {
        let tmp = TempDir::new().unwrap();
        save_notes(tmp.path(), &[]);

        let loaded = load_notes(tmp.path());
        assert!(loaded.is_empty());
    }

    #[test]
    fn save_and_load_notes_with_episode_target() {
        let tmp = TempDir::new().unwrap();
        let notes = vec![UserNote {
            id: "note-1".to_string(),
            text: "Interesting episode".to_string(),
            kind: "private".to_string(),
            target: Some(NoteTarget::Episode {
                episode_id: "ep-123".to_string(),
                position_secs: 42.5,
            }),
            created_at: 1000,
            deleted: false,
            author: "user".to_string(),
        }];

        save_notes(tmp.path(), &notes);
        let loaded = load_notes(tmp.path());

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "note-1");
        assert_eq!(loaded[0].text, "Interesting episode");
    }

    #[test]
    fn save_and_load_notes_with_podcast_target() {
        let tmp = TempDir::new().unwrap();
        let notes = vec![UserNote {
            id: "note-2".to_string(),
            text: "Great podcast".to_string(),
            kind: "public".to_string(),
            target: Some(NoteTarget::Podcast {
                podcast_id: "pod-456".to_string(),
            }),
            created_at: 2000,
            deleted: false,
            author: "user".to_string(),
        }];

        save_notes(tmp.path(), &notes);
        let loaded = load_notes(tmp.path());

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "note-2");
    }

    #[test]
    fn load_nonexistent_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let loaded = load_notes(tmp.path());
        assert!(loaded.is_empty());
    }
}
