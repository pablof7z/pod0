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
fn action_ids_match_documented_strings() {
    assert_eq!(ACTION_CLIP_CREATE, "podcast.clip.create");
    assert_eq!(ACTION_CLIP_DELETE, "podcast.clip.delete");
    assert_eq!(ACTION_CLIP_AUTO_SNIP, "podcast.clip.auto_snip");
    assert_eq!(ACTION_CLIP_RESOLVE_QUOTE, "podcast.clip.resolve_quote");
}
#[test]
fn create_action_round_trips_with_title() {
    let action = ClipAction::Create {
        episode_id: "ep-1".into(),
        start_secs: 10.0,
        end_secs: 70.0,
        title: Some("Marcus on retrieval".into()),
        source: None,
        transcript_text: None,
        client_clip_id: None,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"create""#));
    assert!(json.contains(r#""episode_id":"ep-1""#));
    assert!(json.contains(r#""start_secs":10.0"#));
    assert!(json.contains(r#""end_secs":70.0"#));
    assert!(json.contains(r#""title":"Marcus on retrieval""#));
    let decoded: ClipAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn create_action_omits_none_title() {
    let action = ClipAction::Create {
        episode_id: "ep-1".into(),
        start_secs: 10.0,
        end_secs: 70.0,
        title: None,
        source: Some("agent".into()),
        transcript_text: Some("quoted span".into()),
        client_clip_id: Some("550e8400-e29b-41d4-a716-446655440000".into()),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(!json.contains("\"title\""));
    assert!(json.contains(r#""source":"agent""#));
    assert!(json.contains(r#""transcript_text":"quoted span""#));
    assert!(json.contains(r#""client_clip_id":"550e8400-e29b-41d4-a716-446655440000""#));
    let decoded: ClipAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn delete_action_round_trips() {
    let action = ClipAction::Delete {
        clip_id: "clip-1".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"delete""#));
    assert!(json.contains(r#""clip_id":"clip-1""#));
    let decoded: ClipAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn auto_snip_action_round_trips() {
    let action = ClipAction::AutoSnip {
        episode_id: "ep-1".into(),
        position_secs: 100.0,
        source: None,
        client_clip_id: None,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"auto_snip""#));
    assert!(json.contains(r#""episode_id":"ep-1""#));
    assert!(json.contains(r#""position_secs":100.0"#));
    let decoded: ClipAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn resolve_quote_action_round_trips() {
    let action = ClipAction::ResolveQuote {
        episode_id: "ep-1".into(),
        position_secs: 100.0,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"resolve_quote""#));
    assert!(json.contains(r#""episode_id":"ep-1""#));
    assert!(json.contains(r#""position_secs":100.0"#));
    let decoded: ClipAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn execute_emits_dispatch_host_op() {
    let action = ClipAction::Delete {
        clip_id: "clip-7".into(),
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    ClipActionModule.execute(action, "corr-1", &|cmd| {
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
    assert_eq!(v["ns"], "podcast.clip");
    assert_eq!(v["action"]["op"], "delete");
    assert_eq!(v["action"]["clip_id"], "clip-7");
}
