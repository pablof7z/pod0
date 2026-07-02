//! Host-op handler for identity actions (`podcast.identity.*`).
//!
//! Thin wrapper around [`IdentityStore`] that routes incoming action JSON,
//! mutates the store, bumps the shared `rev` counter, and returns a
//! `{"ok":true}` / `{"ok":false,"error":"..."}` envelope.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::store::identity::IdentityStore;
use crate::nmp_dispatch::{activate_local_signer_in_kernel, remove_account_from_kernel};
use nmp_native_runtime::NmpApp;

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
    pub app: *mut NmpApp,
    pub identity: Arc<Mutex<IdentityStore>>,
    pub rev: Arc<AtomicU64>,
    pub snapshot_signal: Option<SnapshotUpdateSignal>,
    /// The `podcast.identity` domain rev counter. Bumped alongside the global
    /// rev so the `podcast.identity` typed sidecar fires its push delta on an
    /// identity mutation. `None` in tests that don't exercise the push path.
    pub identity_domain_rev: Option<Arc<AtomicU64>>,
}

impl IdentityHandler {
    pub fn new(identity: Arc<Mutex<IdentityStore>>, rev: Arc<AtomicU64>) -> Self {
        Self {
            app: std::ptr::null_mut(),
            identity,
            rev,
            snapshot_signal: None,
            identity_domain_rev: None,
        }
    }

    pub fn with_app(mut self, app: *mut NmpApp) -> Self {
        self.app = app;
        self
    }

    pub fn with_snapshot_signal(mut self, signal: SnapshotUpdateSignal) -> Self {
        self.snapshot_signal = Some(signal);
        self
    }

    /// Wire the `podcast.identity` domain rev so identity mutations advance the
    /// per-domain push delta (in addition to the global rev).
    pub fn with_domain_rev(mut self, domain_rev: Arc<AtomicU64>) -> Self {
        self.identity_domain_rev = Some(domain_rev);
        self
    }

    /// Bump the snapshot rev and — when a signal is wired — tell NMP-core to
    /// re-emit the snapshot projection. Without the signal the rev bump alone
    /// invalidates the snapshot cache; the next NMP-core event will carry the
    /// fresh identity. With the signal a dedicated `MarkChangedSinceEmit` is
    /// posted so a fresh push frame arrives even if no other event fires.
    ///
    /// The identity domain rev (when wired) is advanced first so a consumer
    /// reading the global-rev frame observes the matching `podcast.identity`
    /// delta.
    fn bump_rev(&self) {
        if let Some(ref domain_rev) = self.identity_domain_rev {
            domain_rev.fetch_add(1, Ordering::Relaxed);
        }
        match self.snapshot_signal {
            Some(ref signal) => signal.bump(),
            None => { self.rev.fetch_add(1, Ordering::Relaxed); }
        }
    }

    pub fn handle(&self, action: IdentityAction) -> serde_json::Value {
        match action {
            IdentityAction::ImportNsec { nsec } => match self.identity.lock() {
                Ok(mut id) => match id.import_nsec(&nsec) {
                    Ok(()) => {
                        let secret_hex = id.secret_hex.clone();
                        drop(id);
                        if let Some(secret_hex) = secret_hex.as_deref() {
                            activate_local_signer_in_kernel(self.app, secret_hex);
                        }
                        self.bump_rev();
                        serde_json::json!({"ok": true})
                    }
                    Err(e) => serde_json::json!({"ok": false, "error": e}),
                },
                Err(_) => serde_json::json!({"ok": false, "error": "identity lock poisoned"}),
            },
            IdentityAction::Generate => match self.identity.lock() {
                Ok(mut id) => match id.generate() {
                    Ok(()) => {
                        let secret_hex = id.secret_hex.clone();
                        drop(id);
                        if let Some(secret_hex) = secret_hex.as_deref() {
                            activate_local_signer_in_kernel(self.app, secret_hex);
                        }
                        self.bump_rev();
                        serde_json::json!({"ok": true})
                    }
                    Err(e) => serde_json::json!({"ok": false, "error": e}),
                },
                Err(_) => serde_json::json!({"ok": false, "error": "identity lock poisoned"}),
            },
            IdentityAction::Clear => match self.identity.lock() {
                Ok(mut id) => {
                    let pubkey_hex = id.pubkey_hex.clone();
                    id.clear();
                    drop(id);
                    if let Some(pubkey_hex) = pubkey_hex.as_deref() {
                        remove_account_from_kernel(self.app, pubkey_hex);
                    }
                    self.bump_rev();
                    serde_json::json!({"ok": true})
                }
                Err(_) => serde_json::json!({"ok": false, "error": "identity lock poisoned"}),
            },
            IdentityAction::FetchProfile => {
                // Stub — relay wiring tracked in docs/BACKLOG.md
                serde_json::json!({"ok": true, "status": "nostr_pending"})
            }
        }
    }
}
