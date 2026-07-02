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
    let je = after
        .find(concat!(r#"""#, ", correlation_id:"))
        .expect("json end");
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
fn activate_action_round_trips() {
    let action = VoiceAction::Activate;
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"activate"}"#);
    let decoded: VoiceAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn deactivate_action_round_trips() {
    let action = VoiceAction::Deactivate;
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"deactivate"}"#);
    let decoded: VoiceAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn speak_action_round_trips_with_voice_id() {
    let action = VoiceAction::Speak {
        text: "hello world".into(),
        voice_id: Some("rachel".into()),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"speak""#));
    assert!(json.contains(r#""text":"hello world""#));
    assert!(json.contains(r#""voice_id":"rachel""#));
    let decoded: VoiceAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn speak_action_omits_none_voice_id() {
    let action = VoiceAction::Speak {
        text: "hi".into(),
        voice_id: None,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"speak","text":"hi"}"#);
}
#[test]
fn stop_and_set_voice_round_trip() {
    assert_eq!(
        serde_json::to_string(&VoiceAction::Stop).expect("encode"),
        r#"{"op":"stop"}"#
    );
    let sv = VoiceAction::SetVoice {
        voice_id: "rachel".into(),
    };
    let json = serde_json::to_string(&sv).expect("encode");
    assert_eq!(json, r#"{"op":"set_voice","voice_id":"rachel"}"#);
    let decoded: VoiceAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, sv);
}
#[test]
fn execute_emits_dispatch_host_op() {
    let action = VoiceAction::Activate;
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    VoiceActionModule
        .execute(
            &nmp_core::substrate::ActionContext::default(),
            action,
            "corr-1",
            &|cmd| {
                commands.lock().unwrap().push(cmd);
            },
        )
        .expect("execute ok");
    let commands = commands.into_inner().unwrap();
    assert_eq!(commands.len(), 1);
    let ActorCommand::Protocol(_) = &commands[0] else {
        panic!("expected Protocol command");
    };
    let (action_json, correlation_id) = extract_host_op_parts(&commands[0]);
    assert_eq!(correlation_id.as_str(), "corr-1");
    let v: serde_json::Value = serde_json::from_str(&action_json).expect("json");
    assert_eq!(v["ns"], "podcast.voice");
    assert_eq!(v["action"]["op"], "activate");
}
