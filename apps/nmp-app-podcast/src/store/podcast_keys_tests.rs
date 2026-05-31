use super::*;

#[test]
fn generate_key_returns_32_bytes_and_stores_them() {
    let mut store = PodcastKeyStore::new();
    let sk = store.generate_key("pod-1");
    assert_eq!(sk.len(), 32);
    let fetched = store.get_key("pod-1").expect("stored");
    assert_eq!(fetched, &sk);
}
#[test]
fn pubkey_hex_is_64_chars_lowercase_hex() {
    let mut store = PodcastKeyStore::new();
    store.generate_key("pod-1");
    let pk = store.pubkey_hex("pod-1").expect("derived");
    assert_eq!(pk.len(), 64);
    assert!(pk.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}
#[test]
fn pubkey_hex_is_deterministic_per_secret() {
    let mut store_a = PodcastKeyStore::new();
    let mut store_b = PodcastKeyStore::new();
    let sk = store_a.generate_key("pod-1");
    // Inject the same secret into a second store to compare.
    store_b.keys.insert("pod-1".into(), sk);
    assert_eq!(store_a.pubkey_hex("pod-1"), store_b.pubkey_hex("pod-1"));
}
#[test]
fn different_secrets_produce_different_pubkeys() {
    let mut store = PodcastKeyStore::new();
    store.generate_key("pod-a");
    store.generate_key("pod-b");
    let a = store.pubkey_hex("pod-a").expect("a");
    let b = store.pubkey_hex("pod-b").expect("b");
    // 256 bits of state; collision probability is astronomical.
    assert_ne!(a, b);
}
#[test]
fn remove_key_clears_lookup() {
    let mut store = PodcastKeyStore::new();
    store.generate_key("pod-1");
    store.remove_key("pod-1");
    assert!(store.get_key("pod-1").is_none());
    assert!(store.pubkey_hex("pod-1").is_none());
}
#[test]
fn generate_key_overwrites_existing() {
    let mut store = PodcastKeyStore::new();
    let first = store.generate_key("pod-1");
    let second = store.generate_key("pod-1");
    // Replaces; the second value is what's now stored.
    assert_eq!(store.get_key("pod-1"), Some(&second));
    // And the two generated values differ (UUID v4 random).
    assert_ne!(first, second);
}
#[test]
fn iter_pubkeys_returns_every_known_podcast() {
    let mut store = PodcastKeyStore::new();
    store.generate_key("pod-a");
    store.generate_key("pod-b");
    let mut pairs = store.iter_pubkeys();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].0, "pod-a");
    assert_eq!(pairs[1].0, "pod-b");
    assert_ne!(pairs[0].1, pairs[1].1);
}
#[test]
fn get_key_returns_none_for_unknown_podcast() {
    let store = PodcastKeyStore::new();
    assert!(store.get_key("never-generated").is_none());
    assert!(store.pubkey_hex("never-generated").is_none());
}

// --- M6: hex codec coverage (shared by the Swift Keychain migration) ---

#[test]
fn secret_hex_round_trips_through_hex_to_secret() {
    let mut store = PodcastKeyStore::new();
    let sk = store.generate_key("pod-1");
    let hex = secret_to_hex(&sk);
    assert_eq!(hex.len(), 64);
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    assert_eq!(hex_to_secret(&hex), Some(sk));
}

#[test]
fn hex_to_secret_rejects_malformed_input() {
    assert_eq!(hex_to_secret("deadbeef"), None, "too short");
    assert_eq!(hex_to_secret(&"g".repeat(64)), None, "non-hex chars");
    assert_eq!(hex_to_secret(&"a".repeat(63)), None, "odd length");
}

// --- M6: disk persistence (the `keys_persist_and_reload` proof referenced by
//     the headless `key_persistence` scenario) ---

#[test]
fn keys_persist_and_reload() {
    let dir = tempfile::tempdir().expect("tempdir");

    // First "session": bind to the dir, mint two keys.
    let (pk_a, pk_b) = {
        let mut store = PodcastKeyStore::new();
        assert_eq!(store.set_data_dir(dir.path().to_path_buf()), 0, "empty dir");
        store.generate_key("pod-a");
        store.generate_key("pod-b");
        (
            store.pubkey_hex("pod-a").expect("a"),
            store.pubkey_hex("pod-b").expect("b"),
        )
    };

    // Second "session": fresh store bound to the same dir reloads both keys
    // and re-derives the identical pubkeys.
    let mut reloaded = PodcastKeyStore::new();
    assert_eq!(reloaded.set_data_dir(dir.path().to_path_buf()), 2, "loaded 2");
    assert_eq!(reloaded.pubkey_hex("pod-a").as_deref(), Some(pk_a.as_str()));
    assert_eq!(reloaded.pubkey_hex("pod-b").as_deref(), Some(pk_b.as_str()));
}

#[test]
fn save_creates_missing_data_dir_so_key_survives_reload() {
    // Regression: `save` must `create_dir_all` before writing, exactly like
    // `persistence::save`. The bound directory does NOT exist yet (the iOS
    // shell binds a per-app data dir that may not be created until first
    // write), so without the `create_dir_all` the atomic write to
    // `<dir>/podcast-keys.json.tmp` fails silently and the secret is lost on
    // the next restart. Binding to `tempdir()` directly would NOT catch this
    // (that path already exists), so we bind to a not-yet-created subdir.
    let root = tempfile::tempdir().expect("tempdir");
    let data_dir = root.path().join("not").join("yet").join("created");
    assert!(!data_dir.exists(), "precondition: data dir must not exist yet");

    let pk = {
        let mut store = PodcastKeyStore::new();
        // Nonexistent dir loads nothing.
        assert_eq!(store.set_data_dir(data_dir.clone()), 0, "nothing to load");
        store.generate_key("pod-a");
        // The write must have actually landed on disk, creating the dir.
        assert!(
            data_dir.join(PODCAST_KEYS_FILE).exists(),
            "save must create the missing data dir and write the keys file"
        );
        store.pubkey_hex("pod-a").expect("derived")
    };

    // Fresh "session": a new store bound to the same (now-existing) dir must
    // reload the key the first session minted.
    let mut reloaded = PodcastKeyStore::new();
    assert_eq!(reloaded.set_data_dir(data_dir), 1, "reloaded the persisted key");
    assert_eq!(reloaded.pubkey_hex("pod-a").as_deref(), Some(pk.as_str()));
}

#[test]
fn remove_key_persists_deletion() {
    let dir = tempfile::tempdir().expect("tempdir");
    {
        let mut store = PodcastKeyStore::new();
        store.set_data_dir(dir.path().to_path_buf());
        store.generate_key("pod-a");
        store.generate_key("pod-b");
        store.remove_key("pod-a");
    }
    let mut reloaded = PodcastKeyStore::new();
    assert_eq!(reloaded.set_data_dir(dir.path().to_path_buf()), 1, "only pod-b");
    assert!(reloaded.get_key("pod-a").is_none());
    assert!(reloaded.get_key("pod-b").is_some());
}

#[test]
fn persisted_file_matches_swift_wire_contract() {
    // The Swift `PodcastKeysKeychainMigration.PersistedKeys` decoder expects
    // `schema_version` (u32) + `keys: [{podcast_id, secret_hex}]`. If these
    // field names drift the migration silently no-ops, so pin them here.
    let dir = tempfile::tempdir().expect("tempdir");
    let mut store = PodcastKeyStore::new();
    store.set_data_dir(dir.path().to_path_buf());
    store.generate_key("pod-a");

    let bytes = std::fs::read(dir.path().join(PODCAST_KEYS_FILE)).expect("file written");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");

    assert_eq!(v["schema_version"], 1);
    let keys = v["keys"].as_array().expect("keys array");
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["podcast_id"], "pod-a");
    let secret_hex = keys[0]["secret_hex"].as_str().expect("secret_hex string");
    assert_eq!(secret_hex.len(), 64);
    assert!(secret_hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}

#[test]
fn unknown_schema_version_loads_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join(PODCAST_KEYS_FILE),
        br#"{"schema_version":999,"keys":[{"podcast_id":"x","secret_hex":"aa"}]}"#,
    )
    .expect("write");
    let mut store = PodcastKeyStore::new();
    assert_eq!(store.set_data_dir(dir.path().to_path_buf()), 0);
    assert!(store.get_key("x").is_none());
}

#[test]
fn malformed_row_is_dropped_but_batch_survives() {
    let dir = tempfile::tempdir().expect("tempdir");
    let good = "a".repeat(64);
    let body = format!(
        r#"{{"schema_version":1,"keys":[{{"podcast_id":"bad","secret_hex":"nothex"}},{{"podcast_id":"good","secret_hex":"{good}"}}]}}"#
    );
    std::fs::write(dir.path().join(PODCAST_KEYS_FILE), body).expect("write");
    let mut store = PodcastKeyStore::new();
    assert_eq!(store.set_data_dir(dir.path().to_path_buf()), 1, "only good row");
    assert!(store.get_key("bad").is_none());
    assert!(store.get_key("good").is_some());
}

#[test]
fn save_is_noop_without_data_dir() {
    // Pre-bind (unit-test / pre-login) mutations must not panic and must not
    // touch any file.
    let mut store = PodcastKeyStore::new();
    store.generate_key("pod-a"); // would panic/error if save tried to write
    assert!(store.get_key("pod-a").is_some());
}
