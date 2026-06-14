use super::*;

/// Test helper: extract `(action_json, correlation_id)` from an
/// `ActorCommand::Protocol(HostOpCommand { .. })` via its `Debug` output.
/// HostOpCommand fields are private in nmp-core; this avoids direct access.
#[cfg(test)]
#[allow(dead_code)]
fn extract_host_op_parts(cmd: &ActorCommand) -> (String, String) {
    let dbg = format!("{cmd:?}");
    // Debug fmt: Protocol(HostOpCommand { action_json: "{..}", correlation_id: "corr" })
    // The outer string delimiters are literal " in the Debug output; inner " are \".
    let jm = concat!("action_json: ", r#"""#);
    let js = dbg.find(jm).expect("action_json") + jm.len();
    let after = &dbg[js..];
    let je = after.find(concat!(r#"""#, ", correlation_id:")).expect("json end");
    let raw = &after[..je];
    // Unescape \" → " and \\\\ → \\
    let tmp = raw.replace(r#"\\"#, "\x01BSLASH\x01");
    let action_json = tmp.replace(r#"\""#, r#"""#).replace("\x01BSLASH\x01", "\\");
    let cm = concat!("correlation_id: ", r#"""#);
    let cs = dbg.find(cm).expect("corr_id") + cm.len();
    let after_c = &dbg[cs..];
    let ce = after_c.find(concat!(r#"""#, " }")).expect("corr end");
    (action_json, after_c[..ce].to_string())
}

#[test]
fn remember_action_round_trips_with_explicit_source() {
    let a = MemoryAction::Remember {
        key: "preferred_genre".into(),
        value: "technology".into(),
        source: Some("agent".into()),
    };
    let json = serde_json::to_string(&a).expect("encode");
    assert!(json.contains(r#""op":"remember""#));
    assert!(json.contains(r#""source":"agent""#));
    let decoded: MemoryAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}
#[test]
fn remember_action_omits_none_source_on_wire() {
    let a = MemoryAction::Remember {
        key: "k".into(),
        value: "v".into(),
        source: None,
    };
    let json = serde_json::to_string(&a).expect("encode");
    // `skip_serializing_if = "Option::is_none"` keeps the wire shape
    // narrow when the caller wants the default.
    assert!(!json.contains("source"));
    let decoded: MemoryAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}
#[test]
fn remember_action_decodes_without_source_field() {
    // A hand-written call from Settings doesn't include `source`.
    let json = r#"{"op":"remember","key":"k","value":"v"}"#;
    let decoded: MemoryAction = serde_json::from_str(json).expect("decode");
    assert_eq!(
        decoded,
        MemoryAction::Remember {
            key: "k".into(),
            value: "v".into(),
            source: None,
        }
    );
}
#[test]
fn forget_action_round_trips() {
    let a = MemoryAction::Forget {
        key: "preferred_genre".into(),
    };
    let json = serde_json::to_string(&a).expect("encode");
    assert!(json.contains(r#""op":"forget""#));
    assert!(json.contains(r#""key":"preferred_genre""#));
    let decoded: MemoryAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}
#[test]
fn forget_all_action_round_trips_as_bare_op() {
    let a = MemoryAction::ForgetAll;
    let json = serde_json::to_string(&a).expect("encode");
    assert_eq!(json, r#"{"op":"forget_all"}"#);
    let decoded: MemoryAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}
#[test]
fn execute_emits_dispatch_host_op() {
    let action = MemoryAction::Remember {
        key: "k".into(),
        value: "v".into(),
        source: None,
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    MemoryActionModule.execute(action, "corr-7", &|cmd| {
        commands.lock().unwrap().push(cmd);
    })
    .expect("execute ok");
    let commands = commands.into_inner().unwrap();
    assert_eq!(commands.len(), 1);
    let ActorCommand::Protocol(_) = &commands[0]
    else { panic!("expected Protocol command"); };
    let (action_json, correlation_id) = extract_host_op_parts(&commands[0]);
    assert_eq!(correlation_id.as_str(), "corr-7");
    let v: serde_json::Value = serde_json::from_str(&action_json).expect("json");
    assert_eq!(v["ns"], "podcast.memory");
    assert_eq!(v["action"]["op"], "remember");
}
