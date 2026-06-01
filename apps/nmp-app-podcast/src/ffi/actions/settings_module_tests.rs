use super::*;
#[test]
fn set_skip_intervals_round_trips() {
    let action = SettingsAction::SetSkipIntervals { forward_secs: 45.0, backward_secs: 10.0 };
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
fn execute_emits_dispatch_host_op() {
    let action = SettingsAction::SetAutoSkipAds { enabled: false };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    SettingsActionModule::execute(action, "corr-1", &|cmd| {
        commands.lock().unwrap().push(cmd);
    })
    .expect("execute ok");
    let commands = commands.into_inner().unwrap();
    assert_eq!(commands.len(), 1);
    let ActorCommand::DispatchHostOp { action_json, correlation_id } = &commands[0] else {
        panic!("expected DispatchHostOp");
    };
    assert_eq!(correlation_id, "corr-1");
    let v: serde_json::Value = serde_json::from_str(action_json).expect("json");
    assert_eq!(v["op"], "set_auto_skip_ads");
    assert_eq!(v["enabled"], false);
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
    let ActorCommand::DispatchHostOp { action_json, correlation_id } = &commands[1] else {
        panic!("expected DispatchHostOp second, got {:?}", commands[1]);
    };
    assert_eq!(correlation_id, "corr-2");
    let v: serde_json::Value = serde_json::from_str(action_json).expect("json");
    assert_eq!(v["op"], "add_relay");
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
    assert_eq!(v["op"], "set_relay_role");
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
    assert_eq!(v["op"], "remove_relay");
}

