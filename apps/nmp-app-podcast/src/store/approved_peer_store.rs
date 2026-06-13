//! JSON persistence for the kernel-owned approve/block allow-list.
//!
//! ## Purpose
//!
//! The kernel owns the canonical trust decision for Nostr peer access. An
//! **approved** pubkey is explicitly trusted by the user regardless of follow
//! status; a **blocked** pubkey is unconditionally excluded (overrides follow
//! status). The composed trust predicate is:
//!
//! ```text
//! trust(pubkey) = (followed(pubkey) || approved(pubkey)) && !blocked(pubkey)
//! ```
//!
//! ## Shape
//!
//! A single JSON object persisted under the bound data dir:
//! ```json
//! {
//!   "approved": ["<hex>", ...],
//!   "blocked":  ["<hex>", ...]
//! }
//! ```
//!
//! ## D6
//!
//! A missing or corrupt file silently loads as empty state (no crash, no
//! spurious approvals/blocks; the user can re-add them). Write failures leave
//! the in-memory store authoritative for the session.
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// File name written under the bound `data_dir`.
pub const APPROVED_PEER_STORE_FILE: &str = "approved-peers.json";

/// On-disk shape (serialized directly from/into `ApprovedPeerStore`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ApprovedPeerStore {
    /// Explicitly approved hex pubkeys (clears any block for the same key).
    #[serde(default)]
    pub approved: BTreeSet<String>,
    /// Explicitly blocked hex pubkeys (clears any approval for the same key).
    /// A blocked key is NEVER trusted, even when followed.
    #[serde(default)]
    pub blocked: BTreeSet<String>,
}

impl ApprovedPeerStore {
    /// Construct an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark `pubkey_hex` as approved, removing any prior block for that key.
    pub fn approve(&mut self, pubkey_hex: &str) {
        self.blocked.remove(pubkey_hex);
        self.approved.insert(pubkey_hex.to_owned());
    }

    /// Mark `pubkey_hex` as blocked, removing any prior approval for that key.
    /// Block is an absolute override — a blocked+followed pubkey is untrusted.
    pub fn block(&mut self, pubkey_hex: &str) {
        self.approved.remove(pubkey_hex);
        self.blocked.insert(pubkey_hex.to_owned());
    }

    /// Remove an explicit approval without blocking. The pubkey reverts to
    /// follow-only trust (trusted iff followed).
    pub fn remove_approval(&mut self, pubkey_hex: &str) {
        self.approved.remove(pubkey_hex);
    }

    /// Remove an explicit block without approving. The pubkey reverts to
    /// follow-only trust (trusted iff followed).
    pub fn remove_block(&mut self, pubkey_hex: &str) {
        self.blocked.remove(pubkey_hex);
    }

    /// Returns `true` when `pubkey_hex` has an explicit approval.
    pub fn is_approved(&self, pubkey_hex: &str) -> bool {
        self.approved.contains(pubkey_hex)
    }

    /// Returns `true` when `pubkey_hex` has an explicit block.
    pub fn is_blocked(&self, pubkey_hex: &str) -> bool {
        self.blocked.contains(pubkey_hex)
    }
}

// ── Persistence ───────────────────────────────────────────────────────────────

fn store_path(data_dir: &Path) -> PathBuf {
    data_dir.join(APPROVED_PEER_STORE_FILE)
}

/// Load the store from `data_dir`. Returns an empty `ApprovedPeerStore` when
/// the file is absent, empty, or unparseable (D6 — fresh start, not an error).
pub fn load_approved_peer_store(data_dir: &Path) -> ApprovedPeerStore {
    let path = store_path(data_dir);
    let bytes = match std::fs::read(&path) {
        Ok(b) if !b.is_empty() => b,
        _ => return ApprovedPeerStore::new(),
    };
    match serde_json::from_slice::<ApprovedPeerStore>(&bytes) {
        Ok(store) => store,
        Err(e) => {
            eprintln!(
                "[approved_peer_store] failed to parse {}: {e} — starting empty",
                path.display()
            );
            ApprovedPeerStore::new()
        }
    }
}

/// Persist `store` to `data_dir` using the atomic tmp-rename pattern so a
/// crash during write cannot corrupt the existing file.
///
/// D6: write failure is logged but NOT propagated — the in-memory store
/// remains authoritative for the session.
pub fn save_approved_peer_store(
    data_dir: &Path,
    store: &ApprovedPeerStore,
) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(store)?;
    let dest = store_path(data_dir);
    let tmp = dest.with_extension("tmp");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &dest)?;
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const ALICE: &str = "aa11223344556677889900aabbccddeeff00112233445566778899aabbccddee";
    const BOB: &str = "bb11223344556677889900aabbccddeeff00112233445566778899aabbccddee";

    #[test]
    fn approve_sets_approved_and_clears_block() {
        let mut store = ApprovedPeerStore::new();
        // First block, then approve — block must be cleared.
        store.block(ALICE);
        assert!(store.is_blocked(ALICE));
        store.approve(ALICE);
        assert!(store.is_approved(ALICE));
        assert!(!store.is_blocked(ALICE));
    }

    #[test]
    fn block_sets_blocked_and_clears_approval() {
        let mut store = ApprovedPeerStore::new();
        store.approve(ALICE);
        assert!(store.is_approved(ALICE));
        store.block(ALICE);
        assert!(store.is_blocked(ALICE));
        assert!(!store.is_approved(ALICE));
    }

    #[test]
    fn remove_approval_reverts_to_neutral() {
        let mut store = ApprovedPeerStore::new();
        store.approve(ALICE);
        store.remove_approval(ALICE);
        assert!(!store.is_approved(ALICE));
        assert!(!store.is_blocked(ALICE));
    }

    #[test]
    fn remove_block_reverts_to_neutral() {
        let mut store = ApprovedPeerStore::new();
        store.block(ALICE);
        store.remove_block(ALICE);
        assert!(!store.is_approved(ALICE));
        assert!(!store.is_blocked(ALICE));
    }

    #[test]
    fn independent_entries_do_not_interfere() {
        let mut store = ApprovedPeerStore::new();
        store.approve(ALICE);
        store.block(BOB);
        assert!(store.is_approved(ALICE));
        assert!(!store.is_blocked(ALICE));
        assert!(store.is_blocked(BOB));
        assert!(!store.is_approved(BOB));
    }

    #[test]
    fn persist_and_reload_roundtrip() {
        let dir = TempDir::new().unwrap();
        let mut store = ApprovedPeerStore::new();
        store.approve(ALICE);
        store.block(BOB);

        save_approved_peer_store(dir.path(), &store).unwrap();
        let loaded = load_approved_peer_store(dir.path());
        assert_eq!(loaded.approved, store.approved);
        assert_eq!(loaded.blocked, store.blocked);
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        let store = load_approved_peer_store(dir.path());
        assert!(store.approved.is_empty());
        assert!(store.blocked.is_empty());
    }

    #[test]
    fn load_corrupt_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(APPROVED_PEER_STORE_FILE);
        std::fs::write(&path, b"not valid json").unwrap();
        let store = load_approved_peer_store(dir.path());
        assert!(store.approved.is_empty());
        assert!(store.blocked.is_empty());
    }

    #[test]
    fn load_empty_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(APPROVED_PEER_STORE_FILE);
        std::fs::write(&path, b"").unwrap();
        let store = load_approved_peer_store(dir.path());
        assert!(store.approved.is_empty());
        assert!(store.blocked.is_empty());
    }
}
