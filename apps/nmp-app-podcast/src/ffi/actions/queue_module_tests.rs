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
fn add_next_action_round_trips() {
    let action = QueueAction::AddNext {
        episode_id: "ep-7".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"add_next""#));
    assert!(json.contains(r#""episode_id":"ep-7""#));
    let decoded: QueueAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn add_last_action_round_trips() {
    let action = QueueAction::AddLast {
        episode_id: "ep-7".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"add_last""#));
    assert!(json.contains(r#""episode_id":"ep-7""#));
    let decoded: QueueAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn remove_action_round_trips() {
    let action = QueueAction::Remove {
        episode_id: "ep-7".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"remove""#));
    assert!(json.contains(r#""episode_id":"ep-7""#));
    let decoded: QueueAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn clear_action_round_trips() {
    let action = QueueAction::Clear;
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"clear"}"#);
    let decoded: QueueAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn execute_emits_dispatch_host_op() {
    let action = QueueAction::AddNext {
        episode_id: "ep-7".into(),
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    QueueActionModule.execute(action, "corr-1", &|cmd| {
        commands.lock().unwrap().push(cmd);
    })
    .expect("execute ok");
    let commands = commands.into_inner().unwrap();
    assert_eq!(commands.len(), 1);
    let ActorCommand::Protocol(_) = &commands[0]
    else { panic!("expected Protocol command"); };
    let (action_json, correlation_id) = extract_host_op_parts(&commands[0]);
    assert_eq!(correlation_id.as_str(), "corr-1");
    let v: serde_json::Value = serde_json::from_str(&action_json).expect("json");
    assert_eq!(v["ns"], "podcast.queue");
    assert_eq!(v["action"]["op"], "add_next");
    assert_eq!(v["action"]["episode_id"], "ep-7");
}
#[test]
fn namespace_is_podcast_queue() {
    assert_eq!(QueueActionModule::NAMESPACE.as_str(), "podcast.queue");
}
