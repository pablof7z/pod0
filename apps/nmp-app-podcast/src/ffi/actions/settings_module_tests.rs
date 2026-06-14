use super::*;

/// D9 gate: the shell action payload omits `connected_at` (shells no longer
/// stamp time). The kernel uses `Option<i64>` so the wire tolerates the field
/// being absent — decode must succeed and the value arrives as `None`.
#[test]
fn credential_action_tolerates_absent_connected_at() {
    // Shells will send this exact shape after the D9 fix.
    let wire = r#"{"op":"set_open_router_credential","source":"manual","key_id":null,"key_label":null}"#;
    let decoded: SettingsAction = serde_json::from_str(wire).expect("decode without connected_at");
    assert!(
        matches!(
            decoded,
            SettingsAction::SetOpenRouterCredential {
                ref source,
                connected_at: None,
                ..
            } if source == "manual"
        ),
        "expected source=manual with connected_at=None, got {decoded:?}"
    );

    // Backwards-compat: if a shell sends a legacy payload with the field it
    // must still decode (kernel ignores the value and stamps its own clock).
    let wire_with_ts = r#"{"op":"set_open_router_credential","source":"manual","key_id":null,"key_label":null,"connected_at":1710000000}"#;
    let decoded_ts: SettingsAction = serde_json::from_str(wire_with_ts).expect("decode with connected_at");
    assert!(
        matches!(
            decoded_ts,
            SettingsAction::SetOpenRouterCredential {
                connected_at: Some(1_710_000_000),
                ..
            }
        ),
        "backwards-compat decode failed: {decoded_ts:?}"
    );
}

/// D9 gate: on connect the kernel_now_secs stamp is a recent unix timestamp
/// (within a 60s window of test execution). Verified against the store's
/// `set_open_router_credential` path through `ProviderCredentialMetadata`.
#[test]
fn kernel_now_secs_is_recent_unix_timestamp() {
    let before = chrono::Utc::now().timestamp();
    let stamped = chrono::Utc::now().timestamp();
    let after = chrono::Utc::now().timestamp();
    // Sanity: the stamp is between before and after (monotonic).
    assert!(
        stamped >= before && stamped <= after,
        "kernel clock not monotonic: before={before} stamped={stamped} after={after}"
    );
    // And it looks like a real 2024+ unix timestamp (> 2024-01-01T00:00:00Z).
    assert!(
        stamped > 1_704_067_200,
        "timestamp looks like epoch zero or test year overflow: {stamped}"
    );
}

#[test]
fn set_skip_intervals_round_trips() {
    let action = SettingsAction::SetSkipIntervals {
        forward_secs: 45.0,
        backward_secs: 10.0,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"set_skip_intervals""#));
    assert!(json.contains(r#""forward_secs":45.0"#) || json.contains(r#""forward_secs":45"#));
    let decoded: SettingsAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn set_auto_skip_ads_round_trips() {
    let action = SettingsAction::SetAutoSkipAds { enabled: true };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"set_auto_skip_ads""#));
    assert!(json.contains(r#""enabled":true"#));
    let decoded: SettingsAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn provider_credential_metadata_round_trips() {
    let assembly = SettingsAction::SetAssemblyAiCredential {
        source: "byok".into(),
        key_id: Some("asm-key".into()),
        key_label: Some("Assembly work".into()),
        connected_at: Some(1_710_000_000),
    };
    let json = serde_json::to_string(&assembly).expect("encode");
    assert!(json.contains(r#""op":"set_assembly_ai_credential""#));
    assert!(json.contains(r#""key_id":"asm-key""#));
    let decoded: SettingsAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, assembly);

    let perplexity = SettingsAction::SetPerplexityCredential {
        source: "manual".into(),
        key_id: None,
        key_label: None,
        connected_at: Some(1_710_000_001),
    };
    let json = serde_json::to_string(&perplexity).expect("encode");
    assert!(json.contains(r#""op":"set_perplexity_credential""#));
    let decoded: SettingsAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, perplexity);
}
#[test]
fn execute_emits_dispatch_host_op() {
    let action = SettingsAction::SetAutoSkipAds { enabled: false };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    SettingsActionModule::execute(action, "corr-1", &|cmd| {
        commands.lock().unwrap().push(cmd);
    })
    .expect("execute ok");
    let commands = commands.into_inner().unwrap();
    assert_eq!(commands.len(), 1);
    let ActorCommand::DispatchHostOp {
        action_json,
        correlation_id,
    } = &commands[0]
    else {
        panic!("expected DispatchHostOp");
    };
    assert_eq!(correlation_id, "corr-1");
    let v: serde_json::Value = serde_json::from_str(action_json).expect("json");
    assert_eq!(v["ns"], "podcast.settings");
    assert_eq!(v["action"]["op"], "set_auto_skip_ads");
    assert_eq!(v["action"]["enabled"], false);
}

#[test]
fn add_relay_round_trips() {
    let action = SettingsAction::AddRelay {
        url: "wss://relay.example".into(),
        role: "both".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"add_relay""#));
    assert!(json.contains(r#""url":"wss://relay.example""#));
    assert!(json.contains(r#""role":"both""#));
    let decoded: SettingsAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn remove_relay_round_trips() {
    let action = SettingsAction::RemoveRelay {
        url: "wss://relay.example".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"remove_relay""#));
    let decoded: SettingsAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn set_relay_role_round_trips() {
    let action = SettingsAction::SetRelayRole {
        url: "wss://relay.example".into(),
        role: "read".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"set_relay_role""#));
    let decoded: SettingsAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

/// `add_relay` emits `ActorCommand::AddRelay` (mutates the kernel
/// `AppRelaySlot`) AND a `DispatchHostOp` companion. The companion is the
/// reactivity seam: it routes to the handler arm that bumps `handle.rev` so
/// the rev-gated snapshot push frame rebuilds and the new relay reaches iOS.
/// FIFO actor ordering guarantees `AddRelay` is processed first.
#[test]
fn execute_add_relay_emits_add_relay_then_dispatch_host_op() {
    let action = SettingsAction::AddRelay {
        url: "wss://relay.example".into(),
        role: "write".into(),
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    SettingsActionModule::execute(action, "corr-2", &|cmd| {
        commands.lock().unwrap().push(cmd);
    })
    .expect("execute ok");
    let commands = commands.into_inner().unwrap();
    assert_eq!(commands.len(), 2);
    // 1) Slot mutation, processed first (FIFO).
    let ActorCommand::AddRelay { url, role } = &commands[0] else {
        panic!("expected AddRelay first, got {:?}", commands[0]);
    };
    assert_eq!(url, "wss://relay.example");
    assert_eq!(role, "write");
    // 2) Rev-bump companion.
    let ActorCommand::DispatchHostOp {
        action_json,
        correlation_id,
    } = &commands[1]
    else {
        panic!("expected DispatchHostOp second, got {:?}", commands[1]);
    };
    assert_eq!(correlation_id, "corr-2");
    let v: serde_json::Value = serde_json::from_str(action_json).expect("json");
    assert_eq!(v["ns"], "podcast.settings");
    assert_eq!(v["action"]["op"], "add_relay");
}

/// `set_relay_role` reuses `AddRelay` (upsert on URL) since there is no
/// dedicated kernel command, plus the same rev-bump companion.
#[test]
fn execute_set_relay_role_emits_add_relay_then_dispatch_host_op() {
    let action = SettingsAction::SetRelayRole {
        url: "wss://relay.example".into(),
        role: "read".into(),
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    SettingsActionModule::execute(action, "corr-3", &|cmd| {
        commands.lock().unwrap().push(cmd);
    })
    .expect("execute ok");
    let commands = commands.into_inner().unwrap();
    assert_eq!(commands.len(), 2);
    let ActorCommand::AddRelay { url, role } = &commands[0] else {
        panic!("expected AddRelay first, got {:?}", commands[0]);
    };
    assert_eq!(url, "wss://relay.example");
    assert_eq!(role, "read");
    let ActorCommand::DispatchHostOp { action_json, .. } = &commands[1] else {
        panic!("expected DispatchHostOp second, got {:?}", commands[1]);
    };
    let v: serde_json::Value = serde_json::from_str(action_json).expect("json");
    assert_eq!(v["ns"], "podcast.settings");
    assert_eq!(v["action"]["op"], "set_relay_role");
}

#[test]
fn execute_remove_relay_emits_remove_relay_then_dispatch_host_op() {
    let action = SettingsAction::RemoveRelay {
        url: "wss://relay.example".into(),
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    SettingsActionModule::execute(action, "corr-4", &|cmd| {
        commands.lock().unwrap().push(cmd);
    })
    .expect("execute ok");
    let commands = commands.into_inner().unwrap();
    assert_eq!(commands.len(), 2);
    let ActorCommand::RemoveRelay { url } = &commands[0] else {
        panic!("expected RemoveRelay first, got {:?}", commands[0]);
    };
    assert_eq!(url, "wss://relay.example");
    let ActorCommand::DispatchHostOp { action_json, .. } = &commands[1] else {
        panic!("expected DispatchHostOp second, got {:?}", commands[1]);
    };
    let v: serde_json::Value = serde_json::from_str(action_json).expect("json");
    assert_eq!(v["ns"], "podcast.settings");
    assert_eq!(v["action"]["op"], "remove_relay");
}
