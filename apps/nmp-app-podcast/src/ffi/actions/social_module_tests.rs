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
fn publish_profile_round_trips() {
    let action = SocialAction::PublishProfile {
        name: "alice".into(),
        display_name: Some("Alice".into()),
        about: Some("hi".into()),
        picture: Some("https://example.com/a.png".into()),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"publish_profile""#));
    let decoded: SocialAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn publish_profile_omits_absent_optionals() {
    let action = SocialAction::PublishProfile {
        name: "bob".into(),
        display_name: None,
        about: None,
        picture: None,
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(!json.contains("display_name"));
    assert!(!json.contains("about"));
    assert!(!json.contains("picture"));
}

#[test]
fn publish_profile_decodes_minimal_payload() {
    // Only the discriminator + required `name` — mirrors the leanest
    // Swift dispatch.
    let decoded: SocialAction =
        serde_json::from_str(r#"{"op":"publish_profile","name":"carol"}"#).expect("decode");
    assert_eq!(
        decoded,
        SocialAction::PublishProfile {
            name: "carol".into(),
            display_name: None,
            about: None,
            picture: None,
        }
    );
}

#[test]
fn publish_note_round_trips_with_episode_coord() {
    let action = SocialAction::PublishNote {
        content: "hello".into(),
        episode_coord: Some("30311:abc:def".into()),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"publish_note""#));
    assert!(json.contains(r#""episode_coord":"30311:abc:def""#));
    let decoded: SocialAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn publish_note_decodes_without_episode_coord() {
    let decoded: SocialAction =
        serde_json::from_str(r#"{"op":"publish_note","content":"hi"}"#).expect("decode");
    assert_eq!(
        decoded,
        SocialAction::PublishNote {
            content: "hi".into(),
            episode_coord: None,
        }
    );
}

#[test]
fn publish_highlight_round_trips_with_typed_fields() {
    let action = SocialAction::PublishHighlight {
        content: "quote".into(),
        enclosure_url: Some("https://example.com/a.mp3".into()),
        feed_url: Some("https://example.com/feed.xml".into()),
        item_guid: Some("GUID".into()),
        start_sec: Some(1),
        end_sec: Some(2),
        caption: Some("caption".into()),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"publish_highlight""#));
    assert!(json.contains(r#""item_guid":"GUID""#));
    let decoded: SocialAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn publish_highlight_decodes_minimal_payload() {
    let decoded: SocialAction =
        serde_json::from_str(r#"{"op":"publish_highlight","content":"q"}"#).expect("decode");
    assert_eq!(
        decoded,
        SocialAction::PublishHighlight {
            content: "q".into(),
            enclosure_url: None,
            feed_url: None,
            item_guid: None,
            start_sec: None,
            end_sec: None,
            caption: None,
        }
    );
}

#[test]
fn execute_emits_dispatch_host_op() {
    let action = SocialAction::PublishNote {
        content: "hi".into(),
        episode_coord: None,
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    SocialActionModule.execute(action, "corr-1", &|cmd| {
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
    assert_eq!(v["ns"], "podcast.social");
    assert_eq!(v["action"]["op"], "publish_note");
    assert_eq!(v["action"]["content"], "hi");
}
