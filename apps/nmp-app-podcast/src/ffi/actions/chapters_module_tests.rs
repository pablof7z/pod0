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
fn compile_action_round_trips() {
    let action = ChaptersAction::Compile {
        episode_id: "ep-1".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"compile""#));
    assert!(json.contains(r#""episode_id":"ep-1""#));
    let decoded: ChaptersAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn execute_emits_dispatch_host_op_with_payload() {
    let action = ChaptersAction::Compile {
        episode_id: "ep-2".into(),
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    ChaptersActionModule.execute(action, "corr-1", &|cmd| {
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
    assert_eq!(v["ns"], "podcast.chapters");
    assert_eq!(v["action"]["op"], "compile");
    assert_eq!(v["action"]["episode_id"], "ep-2");
}
