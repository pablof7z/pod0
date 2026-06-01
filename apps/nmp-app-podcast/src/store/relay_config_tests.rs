//! Unit tests for the C-ABI relay-config sidecar
//! ([`super::save_relay_config`] / [`super::load_relay_config`]).

use super::*;

fn relays() -> Vec<(String, String)> {
    vec![
        (
            "wss://relay.primal.net".to_string(),
            "both,indexer".to_string(),
        ),
        ("wss://purplepag.es".to_string(), "indexer".to_string()),
    ]
}

#[test]
fn save_then_load_round_trips() {
    let dir = tempfile::tempdir().expect("tempdir");
    let expected = relays();
    save_relay_config(dir.path(), &expected).expect("save succeeds");
    let loaded = load_relay_config(dir.path());
    assert_eq!(loaded, expected, "loaded list must equal saved list");
}

#[test]
fn save_preserves_order_and_roles() {
    let dir = tempfile::tempdir().expect("tempdir");
    let custom = vec![
        ("wss://a.example".to_string(), "read".to_string()),
        ("wss://b.example".to_string(), "write".to_string()),
        ("wss://c.example".to_string(), "both".to_string()),
    ];
    save_relay_config(dir.path(), &custom).expect("save succeeds");
    assert_eq!(load_relay_config(dir.path()), custom);
}

#[test]
fn load_missing_file_returns_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    // No save() call — the sidecar does not exist.
    assert!(
        load_relay_config(dir.path()).is_empty(),
        "missing sidecar must yield an empty list (fall back to seed)"
    );
}

#[test]
fn load_empty_array_returns_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Persisting an explicitly empty list and reloading must yield empty so
    // the caller falls back to the first-install default seed.
    save_relay_config(dir.path(), &[]).expect("save empty succeeds");
    assert!(
        load_relay_config(dir.path()).is_empty(),
        "an empty sidecar array must load as empty"
    );
}

#[test]
fn load_malformed_json_returns_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(RELAY_CONFIG_FILENAME);
    std::fs::write(&path, b"{ this is not valid json").expect("write malformed");
    assert!(
        load_relay_config(dir.path()).is_empty(),
        "unparseable sidecar must degrade to empty, not crash"
    );
}

#[test]
fn save_overwrites_previous_config() {
    let dir = tempfile::tempdir().expect("tempdir");
    save_relay_config(dir.path(), &relays()).expect("first save");
    let replacement = vec![("wss://only.example".to_string(), "both".to_string())];
    save_relay_config(dir.path(), &replacement).expect("second save");
    assert_eq!(
        load_relay_config(dir.path()),
        replacement,
        "save must fully replace the prior sidecar, not append"
    );
}

#[test]
fn on_disk_shape_is_url_role_objects() {
    // Lock the wire format so it stays byte-compatible with the
    // nmp-app-template sidecar (one canonical representation).
    let dir = tempfile::tempdir().expect("tempdir");
    save_relay_config(
        dir.path(),
        &[("wss://r.example".to_string(), "both".to_string())],
    )
    .expect("save succeeds");
    let raw = std::fs::read_to_string(dir.path().join(RELAY_CONFIG_FILENAME))
        .expect("sidecar readable");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid json");
    assert_eq!(parsed[0]["url"], "wss://r.example");
    assert_eq!(parsed[0]["role"], "both");
}
