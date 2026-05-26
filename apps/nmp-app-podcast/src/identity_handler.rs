//! Host-op handler for identity actions (`podcast.identity.*`).
//!
//! Thin wrapper around [`IdentityStore`] that routes incoming action JSON,
//! mutates the store, bumps the shared `rev` counter, and returns a
//! `{"ok":true}` / `{"ok":false,"error":"..."}` envelope.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::store::identity::IdentityStore;

/// Wire enum for all `podcast.identity` namespace actions.
///
/// The `#[serde(tag = "type")]` discriminator matches the JSON `"type"` field
/// so the headless scenario can dispatch e.g.
/// `{"type":"ImportNsec","nsec":"nsec1..."}`.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type")]
pub enum IdentityAction {
    /// Import a secret key from a bech32 `nsec1…` or raw 64-char hex string.
    ImportNsec { nsec: String },
    /// Generate a fresh random keypair and persist it.
    Generate,
    /// Wipe the active identity and delete `identity.json` from disk.
    Clear,
    /// Fetch the active account's Nostr profile (kind:0). Stub — returns
    /// `{"ok":true}` and does nothing else until the NMP relay pipe is wired.
    FetchProfile,
}

/// Stateless handler struct that borrows the shared `IdentityStore` and `rev`
/// from the `PodcastHostOpHandler`.
pub struct IdentityHandler {
    pub identity: Arc<Mutex<IdentityStore>>,
    pub rev: Arc<AtomicU64>,
}

impl IdentityHandler {
    pub fn new(identity: Arc<Mutex<IdentityStore>>, rev: Arc<AtomicU64>) -> Self {
        Self { identity, rev }
    }

    pub fn handle(&self, action: IdentityAction) -> serde_json::Value {
        match action {
            IdentityAction::ImportNsec { nsec } => {
                match self.identity.lock() {
                    Ok(mut id) => match id.import_nsec(&nsec) {
                        Ok(()) => {
                            self.rev.fetch_add(1, Ordering::Relaxed);
                            serde_json::json!({"ok": true})
                        }
                        Err(e) => serde_json::json!({"ok": false, "error": e}),
                    },
                    Err(_) => serde_json::json!({"ok": false, "error": "identity lock poisoned"}),
                }
            }
            IdentityAction::Generate => {
                match self.identity.lock() {
                    Ok(mut id) => match id.generate() {
                        Ok(()) => {
                            self.rev.fetch_add(1, Ordering::Relaxed);
                            serde_json::json!({"ok": true})
                        }
                        Err(e) => serde_json::json!({"ok": false, "error": e}),
                    },
                    Err(_) => serde_json::json!({"ok": false, "error": "identity lock poisoned"}),
                }
            }
            IdentityAction::Clear => {
                match self.identity.lock() {
                    Ok(mut id) => {
                        id.clear();
                        self.rev.fetch_add(1, Ordering::Relaxed);
                        serde_json::json!({"ok": true})
                    }
                    Err(_) => serde_json::json!({"ok": false, "error": "identity lock poisoned"}),
                }
            }
            IdentityAction::FetchProfile => {
                // Stub — relay wiring tracked in docs/BACKLOG.md
                serde_json::json!({"ok": true, "status": "nostr_pending"})
            }
        }
    }
}
