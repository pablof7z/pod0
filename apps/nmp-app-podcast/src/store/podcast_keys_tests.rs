use super::*;

#[test]
fn keys_persist_and_reload() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = PodcastKeyStore::new();
    store.set_data_dir(dir.path());
    store.generate_key("pod-1");
    let pk = store.pubkey_hex("pod-1").unwrap();
    store.save_to_disk();

    // A second store instance bound to the same dir should load the key.
    let mut store2 = PodcastKeyStore::new();
    store2.set_data_dir(dir.path()); // load_from_disk_if_present called here
    assert_eq!(store2.pubkey_hex("pod-1").unwrap(), pk);
}

#[test]
fn load_does_not_overwrite_in_memory_keys() {
    let dir = tempfile::tempdir().unwrap();

    // Store1: generate key and persist.
    let mut store1 = PodcastKeyStore::new();
    store1.set_data_dir(dir.path());
    store1.generate_key("pod-1");
    let pk1_original = store1.pubkey_hex("pod-1").unwrap();
    store1.save_to_disk();

    // Store2: generate a different key for the same podcast_id, then
    // bind the data dir. The in-memory key must NOT be overwritten.
    let mut store2 = PodcastKeyStore::new();
    store2.generate_key("pod-1");
    let pk1_different = store2.pubkey_hex("pod-1").unwrap();
    // Sanity: the two independently-generated keys should differ.
    assert_ne!(pk1_original, pk1_different);
    // Now bind — load must NOT overwrite the in-memory key.
    store2.set_data_dir(dir.path());
    assert_eq!(store2.pubkey_hex("pod-1").unwrap(), pk1_different);
}

#[test]
fn missing_file_is_a_silent_noop() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = PodcastKeyStore::new();
    // No file written — set_data_dir should not panic or error.
    store.set_data_dir(dir.path());
    assert!(store.get_key("pod-1").is_none());
}

#[test]
fn save_noop_when_no_data_dir() {
    let mut store = PodcastKeyStore::new();
    store.generate_key("pod-1");
    // Must not panic — save is a silent no-op without a data dir.
    store.save_to_disk();
}

#[test]
fn save_noop_when_empty_map() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = PodcastKeyStore::new();
    store.set_data_dir(dir.path());
    // Empty map — save is a no-op; no file should be written.
    store.save_to_disk();
    assert!(!dir.path().join(PODCAST_KEYS_FILE).exists());
}

#[test]
fn multiple_keys_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = PodcastKeyStore::new();
    store.set_data_dir(dir.path());
    store.generate_key("pod-a");
    store.generate_key("pod-b");
    store.generate_key("pod-c");
    let pk_a = store.pubkey_hex("pod-a").unwrap();
    let pk_b = store.pubkey_hex("pod-b").unwrap();
    let pk_c = store.pubkey_hex("pod-c").unwrap();
    store.save_to_disk();

    let mut store2 = PodcastKeyStore::new();
    store2.set_data_dir(dir.path());
    assert_eq!(store2.pubkey_hex("pod-a").unwrap(), pk_a);
    assert_eq!(store2.pubkey_hex("pod-b").unwrap(), pk_b);
    assert_eq!(store2.pubkey_hex("pod-c").unwrap(), pk_c);
}

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

// --- M6: hex codec coverage (shared by save_to_disk / load + Swift migration) ---

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

