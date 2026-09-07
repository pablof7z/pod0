use pod0_cli::{CliRequest, HostConfig, Shell};

#[test]
fn cli_creates_then_reopens_a_fresh_authoritative_store() {
    let directory = tempfile::tempdir_in(".").unwrap();
    let store = directory.path().join("pod0.sqlite");
    let create: CliRequest = serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "create_store",
        "path": store.to_string_lossy(),
    }))
    .unwrap();
    let mut creating_shell = Shell::new(HostConfig::empty()).unwrap();

    let created = creating_shell.handle(create);
    assert!(created.ok, "{:?}", created.error);
    drop(creating_shell);

    let open: CliRequest = serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "open_store",
        "path": store.to_string_lossy(),
    }))
    .unwrap();
    let mut reopening_shell = Shell::new(HostConfig::empty()).unwrap();
    let reopened = reopening_shell.handle(open);
    assert!(reopened.ok, "{:?}", reopened.error);
}
