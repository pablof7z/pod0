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
//! * Keys are persisted by Rust as plaintext JSON in
//!   `<data_dir>/podcast-keys.json` so an owned podcast survives an app
//!   restart. The iOS `PodcastKeysKeychainMigration` reads this exact file
//!   on launch and upserts each secret into the Keychain; the JSON file
//!   stays the source of truth until M7 flips the Rust read path to the
//!   Keychain (blocked on PD-019).
//! * The `secret_to_hex` / `hex_to_secret` codec below is the wire form both
//!   the JSON file and the Keychain item use (64 lowercase hex chars).
//! * Pubkey derivation uses real secp256k1 (via the `nostr` crate).
//! * Key generation uses `nostr::Keys::generate()` which delegates to
//!   a cryptographically-random source on every supported platform.
//!
//! ## Wire contract (must match the Swift parser)
//!
//! [`PodcastKeysKeychainMigration.PersistedKeys`] decodes this file:
//!
//! ```json
//! { "schema_version": 1, "keys": [ { "podcast_id": "<uuid>", "secret_hex": "<64-hex>" } ] }
//! ```
//!
//! An unknown `schema_version` makes the Swift side skip the migration
//! silently, so the field names and version below are load-bearing.
//!
//! ## D6
//!
//! Every lookup is total: `None` for missing keys, never panics on
//! poisoned mutexes higher up the stack. Persistence failures degrade
//! silently — the in-memory map stays authoritative for the session.

use std::collections::HashMap;
use std::path::PathBuf;

use nostr::{Keys, SecretKey};
use serde::{Deserialize, Serialize};

/// File name written under the bound `data_dir`. Mirrored by the Swift
/// constant `PodcastKeysKeychainMigration.fileName`.
pub const PODCAST_KEYS_FILE: &str = "podcast-keys.json";

/// On-disk schema version. Bump only with a coordinated Swift change —
/// [`PodcastKeysKeychainMigration.supportedSchemaVersion`] skips unknown
/// versions silently.
pub const PODCAST_KEYS_SCHEMA_VERSION: u32 = 1;

/// One persisted secret row. Field names must match the Swift
/// `PersistedKey` `CodingKeys` (`podcast_id`, `secret_hex`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
struct PersistedKey {
    podcast_id: String,
    secret_hex: String,
}

/// On-disk envelope. Field names must match the Swift `PersistedKeys`
/// `CodingKeys` (`schema_version`, `keys`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
struct PersistedKeys {
    schema_version: u32,
    keys: Vec<PersistedKey>,
}

/// Encode a 32-byte secp256k1 secret scalar as 64 lowercase hex chars —
/// the wire form the Swift Keychain migration writes.
#[must_use]
pub fn secret_to_hex(sk: &[u8; 32]) -> String {
    sk.iter().fold(String::with_capacity(64), |mut s, b| {
        s.push_str(&format!("{b:02x}"));
        s
    })
}

/// Decode a 64-char lowercase-hex secret back into a 32-byte scalar.
/// Returns `None` for any malformed input (wrong length, non-hex).
#[must_use]
pub fn hex_to_secret(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let byte_str = std::str::from_utf8(chunk).ok()?;
        out[i] = u8::from_str_radix(byte_str, 16).ok()?;
    }
    Some(out)
}

/// Per-podcast secret-key store, persisted to `<data_dir>/podcast-keys.json`.
///
/// Keyed by `podcast_id` (UUID hyphenated string — the same form the
/// FFI `PodcastSummary.id` carries) so the action module can resolve a
/// row purely from the wire payload it received.
///
/// A fresh process starts with an empty map; [`set_data_dir`] binds the
/// store to a directory and reloads any saved keys. Every mutation
/// (`generate_key`, `remove_key`) writes through to disk so a key minted
/// during a session survives an app restart.
///
/// [`set_data_dir`]: PodcastKeyStore::set_data_dir
#[derive(Default)]
pub struct PodcastKeyStore {
    keys: HashMap<String, [u8; 32]>,
    /// Directory the JSON file lives in. `None` before `set_data_dir`
    /// (unit tests, pre-bind): mutations stay in-memory and `save` is a
    /// no-op until a directory is bound.
    data_dir: Option<PathBuf>,
}

impl PodcastKeyStore {
    pub fn new() -> Self {
        Self {
            keys: HashMap::new(),
            data_dir: None,
        }
    }

    /// Bind the store to `dir` and reload `podcast-keys.json` from it.
    ///
    /// Returns the number of keys loaded. A missing file (the common
    /// no-owned-podcasts case), malformed JSON, or an unknown schema
    /// version yields zero loaded keys and is not an error (D6). Rows
    /// whose `secret_hex` is not 64-char hex are dropped individually so
    /// one corrupt row can't poison the rest of the batch.
    pub fn set_data_dir(&mut self, dir: PathBuf) -> usize {
        let path = dir.join(PODCAST_KEYS_FILE);
        self.data_dir = Some(dir);

        let Ok(bytes) = std::fs::read(&path) else {
            return 0;
        };
        let Ok(payload) = serde_json::from_slice::<PersistedKeys>(&bytes) else {
            return 0;
        };
        if payload.schema_version != PODCAST_KEYS_SCHEMA_VERSION {
            return 0;
        }
        let mut loaded = 0;
        for row in payload.keys {
            if let Some(sk) = hex_to_secret(&row.secret_hex) {
                self.keys.insert(row.podcast_id, sk);
                loaded += 1;
            }
        }
        loaded
    }

    /// Generate (or replace) a cryptographically-random secp256k1
    /// secret key for `podcast_id`, persisting the updated set. Returns
    /// the raw 32-byte scalar so callers can inspect it without a second
    /// lookup.
    pub fn generate_key(&mut self, podcast_id: &str) -> [u8; 32] {
        let sk = nostr::Keys::generate().secret_key().to_secret_bytes();
        self.keys.insert(podcast_id.to_owned(), sk);
        self.save();
        sk
    }

    pub fn get_key(&self, podcast_id: &str) -> Option<&[u8; 32]> {
        self.keys.get(podcast_id)
    }

    pub fn remove_key(&mut self, podcast_id: &str) {
        if self.keys.remove(podcast_id).is_some() {
            self.save();
        }
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

    /// Serialize the current key set to `<data_dir>/podcast-keys.json`.
    ///
    /// No-op when no `data_dir` is bound (unit tests / pre-bind). Write
    /// failures degrade silently — the in-memory map stays authoritative
    /// for the session (D6). Rows are sorted by `podcast_id` so the file
    /// is deterministic regardless of `HashMap` iteration order.
    ///
    /// The write is atomic (serialize → `podcast-keys.json.tmp` → `rename`),
    /// matching [`crate::store::persistence::save`]. A torn write here would
    /// orphan every owned show permanently, so the rename fence matters more
    /// than for the library file.
    fn save(&self) {
        let Some(dir) = &self.data_dir else { return };
        // Mirror `persistence::save`: ensure the data directory exists before
        // writing. Without this, the first-ever write to a not-yet-created
        // `data_dir` fails silently and the freshly-minted secret is lost on
        // the next restart (D6 keeps the in-memory map for the session only).
        let _ = std::fs::create_dir_all(dir);
        let mut keys: Vec<PersistedKey> = self
            .keys
            .iter()
            .map(|(id, sk)| PersistedKey {
                podcast_id: id.clone(),
                secret_hex: secret_to_hex(sk),
            })
            .collect();
        keys.sort_by(|a, b| a.podcast_id.cmp(&b.podcast_id));
        let payload = PersistedKeys {
            schema_version: PODCAST_KEYS_SCHEMA_VERSION,
            keys,
        };
        let Ok(json) = serde_json::to_vec_pretty(&payload) else {
            return;
        };
        let final_path = dir.join(PODCAST_KEYS_FILE);
        let tmp_path = dir.join(format!("{PODCAST_KEYS_FILE}.tmp"));
        if std::fs::write(&tmp_path, &json).is_err() {
            return;
        }
        let _ = std::fs::rename(&tmp_path, &final_path);
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
            secret_to_hex(sk)
        })
}

#[cfg(test)]
#[path = "podcast_keys_tests.rs"]
mod tests;
