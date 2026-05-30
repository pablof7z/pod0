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
