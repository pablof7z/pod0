//! Per-podcast keypair store for NIP-F4 owned podcast publishing
//! (features #27/#28).
//!
//! Each podcast the user "owns" — i.e. has chosen to publish as a
//! NIP-F4 `kind:10154` show event — gets its own Nostr secret key.
//! The public key derived from it is what appears in the show event,
//! and (collectively) in the user's `kind:10064` author-claim event
//! under the agent identity.
//!
//! ## Scope
//!
//! * Disk persistence via `podcast-keys.json` in the configured data dir.
//!   Keys survive app restarts once `set_data_dir` has been called.
//! * Pubkey derivation uses real secp256k1 (via the `nostr` crate).
//! * Key generation uses `nostr::Keys::generate()` which delegates to
//!   a cryptographically-random source on every supported platform.
//!
//! ## D6
//!
//! Every lookup is total: `None` for missing keys, never panics on
//! poisoned mutexes higher up the stack. Disk I/O failures degrade
//! silently — the in-memory store stays authoritative.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use nostr::{Keys, SecretKey};
use serde::{Deserialize, Serialize};

/// File name of the persisted key store inside the data directory.
pub const PODCAST_KEYS_FILE: &str = "podcast-keys.json";

/// Schema version for `podcast-keys.json`. Bump on incompatible format
/// changes; unknown versions are treated as empty on load.
const KEYS_SCHEMA_VERSION: u32 = 1;

/// On-disk row for a single per-podcast keypair.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedKey {
    podcast_id: String,
    /// 32-byte secret key encoded as 64 lowercase hex characters.
    secret_hex: String,
}

/// On-disk envelope for `podcast-keys.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedKeys {
    schema_version: u32,
    keys: Vec<PersistedKey>,
}

/// In-memory per-podcast secret-key store with optional disk persistence.
///
/// Keyed by `podcast_id` (UUID hyphenated string — the same form the
/// FFI `PodcastSummary.id` carries) so the action module can resolve a
/// row purely from the wire payload it received.
///
/// When `data_dir` is set (via [`Self::set_data_dir`]) all mutations are
/// written through to `<data_dir>/podcast-keys.json` atomically. Call
/// `set_data_dir` once after construction (mirroring the `PodcastStore`
/// lifecycle) to reload persisted keys.
#[derive(Default)]
pub struct PodcastKeyStore {
    keys: HashMap<String, [u8; 32]>,
    data_dir: Option<PathBuf>,
}

impl PodcastKeyStore {
    pub fn new() -> Self {
        Self {
            keys: HashMap::new(),
            data_dir: None,
        }
    }

    /// Bind the store to a persistence directory and load any previously
    /// saved keys. Must be called once, after the `PodcastStore` data dir
    /// is configured, so that `save_to_disk` has a target.
    ///
    /// Calls [`Self::load_from_disk_if_present`] internally; any keys
    /// already in memory are preserved (loaded keys fill in the gaps).
    pub fn set_data_dir(&mut self, path: &Path) {
        self.data_dir = Some(path.to_owned());
        self.load_from_disk_if_present();
    }

    /// Generate (or replace) a cryptographically-random secp256k1
    /// secret key for `podcast_id`. Returns the raw 32-byte scalar so
    /// callers can persist or inspect without a second lookup.
    ///
    /// Does NOT call `save_to_disk` — the caller is responsible for
    /// persisting after mutating (typically immediately after this call).
    pub fn generate_key(&mut self, podcast_id: &str) -> [u8; 32] {
        let sk = nostr::Keys::generate().secret_key().to_secret_bytes();
        self.keys.insert(podcast_id.to_owned(), sk);
        sk
    }

    pub fn get_key(&self, podcast_id: &str) -> Option<&[u8; 32]> {
        self.keys.get(podcast_id)
    }

    pub fn remove_key(&mut self, podcast_id: &str) {
        self.keys.remove(podcast_id);
    }

    /// x-only public key (hex) derived from the stored secp256k1 secret
    /// key for `podcast_id`. Returns `None` when the podcast is unknown.
    pub fn pubkey_hex(&self, podcast_id: &str) -> Option<String> {
        self.keys.get(podcast_id).map(derive_pubkey_hex)
    }

    /// Iterator over `(podcast_id, pubkey_hex)` for every key currently
    /// known to the store. Used by the snapshot builder to populate
    /// `PodcastUpdate.owned_podcasts` and by `publish_author_claim`
    /// to enumerate `p` tags.
    pub fn iter_pubkeys(&self) -> Vec<(String, String)> {
        self.keys
            .iter()
            .map(|(id, sk)| (id.clone(), derive_pubkey_hex(sk)))
            .collect()
    }

    /// Flush the current in-memory keys to `<data_dir>/podcast-keys.json`.
    ///
    /// Uses an atomic write (`.tmp` + `rename`) so a crash mid-write
    /// never produces a corrupt file. Silent no-op when `data_dir` is
    /// unset or the map is empty. Failures are swallowed (D6) — the
    /// in-memory store stays authoritative.
    pub fn save_to_disk(&self) {
        let Some(dir) = self.data_dir.as_ref() else { return; };
        if self.keys.is_empty() {
            return;
        }
        let rows: Vec<PersistedKey> = self
            .keys
            .iter()
            .map(|(id, sk)| PersistedKey {
                podcast_id: id.clone(),
                secret_hex: sk
                    .iter()
                    .fold(String::with_capacity(64), |mut s, b| {
                        s.push_str(&format!("{b:02x}"));
                        s
                    }),
            })
            .collect();
        let payload = PersistedKeys {
            schema_version: KEYS_SCHEMA_VERSION,
            keys: rows,
        };
        let json = match serde_json::to_vec_pretty(&payload) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("[podcast_keys] serialize error: {e}");
                return;
            }
        };
        // Ensure target directory exists.
        let _ = std::fs::create_dir_all(dir);
        let final_path = dir.join(PODCAST_KEYS_FILE);
        let tmp_path = dir.join(format!("{PODCAST_KEYS_FILE}.tmp"));
        if std::fs::write(&tmp_path, &json).is_err() {
            return;
        }
        let _ = std::fs::rename(&tmp_path, &final_path);
    }

    /// Try to load `<data_dir>/podcast-keys.json`. If the file is missing,
    /// returns silently. If it is present but malformed or has an unknown
    /// schema version, logs to stderr and returns without modifying memory.
    ///
    /// Loaded keys are merged into the existing in-memory map using
    /// `entry().or_insert(...)` so keys already in memory are not
    /// overwritten (in-memory state is always authoritative).
    pub fn load_from_disk_if_present(&mut self) {
        let Some(dir) = self.data_dir.as_ref() else { return; };
        let path = dir.join(PODCAST_KEYS_FILE);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                eprintln!("[podcast_keys] read error: {e}");
                return;
            }
        };
        let payload: PersistedKeys = match serde_json::from_slice(&bytes) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[podcast_keys] parse error: {e}");
                return;
            }
        };
        if payload.schema_version != KEYS_SCHEMA_VERSION {
            eprintln!(
                "[podcast_keys] unknown schema version {} — ignoring disk file",
                payload.schema_version
            );
            return;
        }
        for row in payload.keys {
            // Parse 64-char hex string back to 32 bytes.
            let hex = &row.secret_hex;
            if hex.len() != 64 {
                eprintln!("[podcast_keys] invalid secret_hex length for {}", row.podcast_id);
                continue;
            }
            let mut sk_bytes = [0u8; 32];
            let mut ok = true;
            for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
                let byte_str = match std::str::from_utf8(chunk) {
                    Ok(s) => s,
                    Err(_) => { ok = false; break; }
                };
                match u8::from_str_radix(byte_str, 16) {
                    Ok(b) => sk_bytes[i] = b,
                    Err(_) => { ok = false; break; }
                }
            }
            if !ok {
                eprintln!("[podcast_keys] hex decode failed for {}", row.podcast_id);
                continue;
            }
            // Merge: don't overwrite keys already in memory.
            self.keys.entry(row.podcast_id).or_insert(sk_bytes);
        }
    }
}

/// Derive the x-only secp256k1 public key (lowercase hex) from a
/// 32-byte secret key scalar. The fallback branch (invalid scalar) is
/// astronomically unlikely with randomly-generated keys.
fn derive_pubkey_hex(sk: &[u8; 32]) -> String {
    SecretKey::from_slice(sk)
        .map(|sk| Keys::new(sk).public_key().to_hex())
        .unwrap_or_else(|_| {
            // Invalid scalar (< 1 in 2^128 probability with random input).
            // Return the raw bytes as hex so callers still get 64 chars.
            sk.iter().fold(String::with_capacity(64), |mut s, b| {
                s.push_str(&format!("{b:02x}"));
                s
            })
        })
}

#[cfg(test)]
#[path = "podcast_keys_tests.rs"]
mod tests;
