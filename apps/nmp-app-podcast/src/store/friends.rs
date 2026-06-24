//! Friends store — holds the user's trusted friends and their metadata.
//!
//! Friends are Nostr peers that have been explicitly approved by the user.
//! Each friend record includes the public key and optional display metadata.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// A single friend record with metadata.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct FriendRecord {
    pub id: String,
    pub display_name: String,
    pub pubkey_hex: String,
    pub added_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
}

/// Load friends from disk at the designated data directory.
/// Returns an empty vector if the file doesn't exist or is malformed.
pub fn load_friends(data_dir: &Path) -> Vec<FriendRecord> {
    let path = data_dir.join("friends.json");
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Atomically write friends to disk using write-to-temp-then-rename.
/// No-op if the write fails silently (per D6 degradation).
pub fn save_friends(data_dir: &Path, friends: &[FriendRecord]) {
    let json = match serde_json::to_string(friends) {
        Ok(j) => j,
        Err(_) => return,
    };
    let tmp = data_dir.join("friends.json.tmp");
    let dest = data_dir.join("friends.json");
    if std::fs::write(&tmp, &json).is_ok() {
        let _ = std::fs::rename(&tmp, &dest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn save_and_load_empty_friends() {
        let tmp = TempDir::new().unwrap();
        save_friends(tmp.path(), &[]);

        let loaded = load_friends(tmp.path());
        assert!(loaded.is_empty());
    }

    #[test]
    fn save_and_load_friends() {
        let tmp = TempDir::new().unwrap();
        let friends = vec![FriendRecord {
            id: "friend-1".to_string(),
            display_name: "Alice".to_string(),
            pubkey_hex: "abc123def456".to_string(),
            added_at: 1000,
            avatar_url: Some("https://example.com/alice.png".to_string()),
            about: Some("A friend".to_string()),
        }];

        save_friends(tmp.path(), &friends);
        let loaded = load_friends(tmp.path());

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "friend-1");
        assert_eq!(loaded[0].display_name, "Alice");
        assert_eq!(loaded[0].pubkey_hex, "abc123def456");
    }

    #[test]
    fn load_nonexistent_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let loaded = load_friends(tmp.path());
        assert!(loaded.is_empty());
    }
}
