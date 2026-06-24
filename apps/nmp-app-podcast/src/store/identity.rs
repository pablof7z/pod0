//! Rust-owned identity store — holds the active Nostr secret key and derived
//! public-key projections.
//!
//! ## Design
//!
//! * Owns the secret key as a hex string so it can be serialised to disk
//!   without pulling in a custom `Serialize` impl for `nostr::SecretKey`.
//! * Derives `pubkey_hex` and `npub` lazily on import/load rather than on
//!   every snapshot tick.
//! * `save_to_disk` / `load_from_disk` use a write-to-temp-then-rename
//!   strategy so a crash mid-write never corrupts the on-disk file.
//!
//! ## D6
//!
//! Every failure path (disk I/O, mutex, key parse) degrades silently —
//! the caller always gets either a populated `IdentityStore` or a fully-`None`
//! one; no panics or unwraps in production code paths.

use std::path::{Path, PathBuf};

use nostr::nips::nip19::ToBech32;

/// In-memory representation of the active Nostr identity.
///
/// All fields are `Option` because the store starts empty and is populated
/// either by `import_nsec` / `generate` or by a cold-start `load_from_disk`
/// call from `set_data_dir`.
pub struct IdentityStore {
    /// 32-byte secret key as lowercase hex (64 chars).
    pub secret_hex: Option<String>,
    /// Derived 32-byte x-only pubkey as lowercase hex (64 chars).
    pub pubkey_hex: Option<String>,
    /// bech32 `npub1…` encoding of `pubkey_hex`.
    pub npub: Option<String>,
    pub display_name: Option<String>,
    pub picture_url: Option<String>,
    pub name: Option<String>,
    pub about: Option<String>,
    pub data_dir: Option<PathBuf>,
}

/// On-disk JSON schema for the identity file.
#[derive(serde::Deserialize, serde::Serialize)]
struct IdentityFile {
    schema_version: u32,
    secret_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    picture_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    about: Option<String>,
}

impl IdentityStore {
    pub fn new() -> Self {
        Self {
            secret_hex: None,
            pubkey_hex: None,
            npub: None,
            display_name: None,
            picture_url: None,
            name: None,
            about: None,
            data_dir: None,
        }
    }

    /// Bind the store to a persistence directory and load any previously-saved
    /// identity. If `identity.json` exists and is parseable the derived key
    /// fields (`pubkey_hex`, `npub`) are filled in from the stored secret.
    pub fn set_data_dir(&mut self, path: &Path) {
        self.data_dir = Some(path.to_owned());
        if let Some(loaded) = Self::load_from_disk(path) {
            self.secret_hex = loaded.secret_hex;
            self.pubkey_hex = loaded.pubkey_hex;
            self.npub = loaded.npub;
            self.display_name = loaded.display_name;
            self.picture_url = loaded.picture_url;
            self.name = loaded.name;
            self.about = loaded.about;
        }
    }

    /// Import a secret key from either a bech32 `nsec1…` string or a
    /// raw 64-char lowercase hex string. On success, `secret_hex`,
    /// `pubkey_hex`, and `npub` are all populated and the identity is
    /// persisted to disk (if a `data_dir` has been set).
    pub fn import_nsec(&mut self, nsec_or_hex: &str) -> Result<(), String> {
        let keys = nostr::Keys::parse(nsec_or_hex).map_err(|e| format!("invalid key: {e}"))?;
        self.populate_from_keys(&keys);
        self.save_to_disk();
        Ok(())
    }

    /// Generate a fresh cryptographically-random Nostr keypair, store the
    /// derived fields, and persist to disk.
    pub fn generate(&mut self) -> Result<(), String> {
        let keys = nostr::Keys::generate();
        self.populate_from_keys(&keys);
        self.save_to_disk();
        Ok(())
    }

    /// Mirror the just-published kind:0 profile fields into the in-memory store
    /// and persist them to disk.
    ///
    /// Called immediately after a successful `publish_profile_via_nmp` dispatch
    /// so the local `AccountSummary` projection reflects what the user published
    /// without waiting for a relay echo (optimistic-but-correct, like
    /// `agent_note_responder`'s projection-slot update). Only fields that were
    /// actually set in the payload are applied — `None` leaves the existing value
    /// untouched so the user cannot accidentally null-out a field they left blank.
    ///
    /// Note: the NMP field name for picture is `picture` (payload) which maps to
    /// `picture_url` (store). `display_name` maps 1:1.
    pub fn apply_profile(
        &mut self,
        display_name: Option<String>,
        picture_url: Option<String>,
        name: Option<String>,
        about: Option<String>,
    ) {
        if let Some(v) = display_name {
            self.display_name = Some(v);
        }
        if let Some(v) = picture_url {
            self.picture_url = Some(v);
        }
        if let Some(v) = name {
            self.name = Some(v);
        }
        if let Some(v) = about {
            self.about = Some(v);
        }
        self.save_to_disk();
    }

    /// Wipe all key fields and delete `identity.json` from disk (if the
    /// store has a `data_dir`). Display name and picture are also cleared
    /// so the projection returns `None` for `active_account`.
    pub fn clear(&mut self) {
        self.secret_hex = None;
        self.pubkey_hex = None;
        self.npub = None;
        self.display_name = None;
        self.picture_url = None;
        self.name = None;
        self.about = None;
        if let Some(dir) = &self.data_dir {
            let file = dir.join("identity.json");
            let _ = std::fs::remove_file(file);
        }
    }

    /// Atomically write `identity.json` to `data_dir`. Write-to-temp-then-
    /// rename so a crash mid-write never corrupts the existing file. No-op if
    /// `data_dir` is not set or `secret_hex` is `None`.
    pub fn save_to_disk(&self) {
        let (Some(dir), Some(secret_hex)) = (&self.data_dir, &self.secret_hex) else {
            return;
        };
        let record = IdentityFile {
            schema_version: 1,
            secret_hex: secret_hex.clone(),
            display_name: self.display_name.clone(),
            picture_url: self.picture_url.clone(),
            name: self.name.clone(),
            about: self.about.clone(),
        };
        let json = match serde_json::to_string(&record) {
            Ok(j) => j,
            Err(_) => return,
        };
        let tmp = dir.join("identity.json.tmp");
        let dest = dir.join("identity.json");
        if std::fs::write(&tmp, &json).is_ok() {
            let _ = std::fs::rename(&tmp, &dest);
        }
    }

    /// Read `identity.json` from `data_dir`, parse it, and derive the
    /// public-key fields. Returns `None` if the file does not exist, is
    /// corrupt, or carries an invalid secret key.
    pub fn load_from_disk(data_dir: &Path) -> Option<Self> {
        let path = data_dir.join("identity.json");
        let bytes = std::fs::read(&path).ok()?;
        let record: IdentityFile = serde_json::from_slice(&bytes).ok()?;
        let keys = nostr::Keys::parse(&record.secret_hex).ok()?;
        let mut store = Self::new();
        store.data_dir = Some(data_dir.to_owned());
        store.display_name = record.display_name;
        store.picture_url = record.picture_url;
        store.name = record.name;
        store.about = record.about;
        store.populate_from_keys(&keys);
        Some(store)
    }

    // ---------------------------------------------------------------------------
    // Private helpers
    // ---------------------------------------------------------------------------

    /// Populate `secret_hex`, `pubkey_hex`, and `npub` from a `nostr::Keys`
    /// instance. Silently leaves the fields as `None` if bech32 encoding
    /// fails (that branch is unreachable with a valid key in practice).
    fn populate_from_keys(&mut self, keys: &nostr::Keys) {
        self.secret_hex = Some(keys.secret_key().to_secret_bytes().iter().fold(
            String::with_capacity(64),
            |mut s, b| {
                s.push_str(&format!("{:02x}", b));
                s
            },
        ));
        let pubkey = keys.public_key();
        self.pubkey_hex = Some(pubkey.to_hex());
        // `ToBech32::to_bech32()` returns `Result<String, Infallible>` for
        // `PublicKey` in nostr 0.44 — the unwrap is safe.
        self.npub = Some(pubkey.to_bech32().expect("PublicKey bech32 is infallible"));
    }
}

impl Default for IdentityStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const TEST_NSEC: &str = "nsec1cdxlq0ckkqeuauhzqaduugmrjpwuk3cgwq37ef2nvzje8at26lwqapk9us";
    const TEST_PUBKEY_HEX: &str =
        "c7f5c9fc41894086a2fd8c3e542c1d6e6beeb2175ba41813de38bd02936bd4ff";
    const TEST_NPUB: &str = "npub1cl6unlzp39qgdgha3sl9gtqade47avshtwjpsy778z7s9ymt6nls2thmtl";

    #[test]
    fn import_nsec_populates_all_fields() {
        let mut store = IdentityStore::new();
        store.import_nsec(TEST_NSEC).unwrap();
        assert_eq!(store.pubkey_hex.as_deref().unwrap(), TEST_PUBKEY_HEX);
        assert_eq!(store.npub.as_deref().unwrap(), TEST_NPUB);
        assert!(store.secret_hex.is_some());
    }

    #[test]
    fn import_hex_also_works() {
        let hex = "c34df03f16b033cef2e2075bce2363905dcb47087023eca55360a593f56ad7dc";
        let mut store = IdentityStore::new();
        store.import_nsec(hex).unwrap();
        assert_eq!(store.pubkey_hex.as_deref().unwrap(), TEST_PUBKEY_HEX);
    }

    #[test]
    fn generate_produces_valid_npub() {
        let mut store = IdentityStore::new();
        store.generate().unwrap();
        let npub = store.npub.as_deref().unwrap();
        assert!(npub.starts_with("npub1"), "npub should start with 'npub1'");
        assert_eq!(npub.len(), 63, "npub should be 63 chars");
    }

    #[test]
    fn clear_wipes_all_fields() {
        let mut store = IdentityStore::new();
        store.import_nsec(TEST_NSEC).unwrap();
        store.clear();
        assert!(store.secret_hex.is_none());
        assert!(store.pubkey_hex.is_none());
        assert!(store.npub.is_none());
    }

    #[test]
    fn save_and_load_round_trip() {
        let tmp = TempDir::new().unwrap();
        let mut store = IdentityStore::new();
        store.data_dir = Some(tmp.path().to_owned());
        store.import_nsec(TEST_NSEC).unwrap();

        let loaded = IdentityStore::load_from_disk(tmp.path()).unwrap();
        assert_eq!(loaded.pubkey_hex, store.pubkey_hex);
        assert_eq!(loaded.npub, store.npub);
        assert_eq!(loaded.secret_hex, store.secret_hex);
    }

    #[test]
    fn load_nonexistent_returns_none() {
        let tmp = TempDir::new().unwrap();
        assert!(IdentityStore::load_from_disk(tmp.path()).is_none());
    }

    #[test]
    fn apply_profile_sets_display_name_and_picture_url() {
        let mut store = IdentityStore::new();
        store.import_nsec(TEST_NSEC).unwrap();
        store.apply_profile(Some("Alice".into()), Some("https://example.com/a.png".into()), None, None);
        assert_eq!(store.display_name.as_deref(), Some("Alice"));
        assert_eq!(store.picture_url.as_deref(), Some("https://example.com/a.png"));
    }

    #[test]
    fn apply_profile_none_leaves_existing_fields_intact() {
        let mut store = IdentityStore::new();
        store.import_nsec(TEST_NSEC).unwrap();
        store.display_name = Some("Existing".into());
        store.picture_url = Some("https://example.com/old.png".into());
        // Neither field is set — existing values must survive.
        store.apply_profile(None, None, None, None);
        assert_eq!(store.display_name.as_deref(), Some("Existing"));
        assert_eq!(store.picture_url.as_deref(), Some("https://example.com/old.png"));
    }

    #[test]
    fn apply_profile_persists_to_disk_and_survives_reload() {
        let tmp = TempDir::new().unwrap();
        let mut store = IdentityStore::new();
        store.data_dir = Some(tmp.path().to_owned());
        store.import_nsec(TEST_NSEC).unwrap();
        store.apply_profile(Some("Persisted Name".into()), Some("https://pic.example.com/p.png".into()), Some("Full Name".into()), Some("About me".into()));

        let reloaded = IdentityStore::load_from_disk(tmp.path()).unwrap();
        assert_eq!(reloaded.display_name.as_deref(), Some("Persisted Name"));
        assert_eq!(reloaded.picture_url.as_deref(), Some("https://pic.example.com/p.png"));
        assert_eq!(reloaded.name.as_deref(), Some("Full Name"));
        assert_eq!(reloaded.about.as_deref(), Some("About me"));
    }

    #[test]
    fn set_data_dir_loads_existing_identity() {
        let tmp = TempDir::new().unwrap();
        // Save first.
        let mut store1 = IdentityStore::new();
        store1.data_dir = Some(tmp.path().to_owned());
        store1.import_nsec(TEST_NSEC).unwrap();

        // Create a fresh store and bind to same dir.
        let mut store2 = IdentityStore::new();
        store2.set_data_dir(tmp.path());
        assert_eq!(store2.npub, store1.npub);
    }
}
