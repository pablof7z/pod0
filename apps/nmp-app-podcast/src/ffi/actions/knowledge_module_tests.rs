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
fn knowledge_action_ids_match_documented_strings() {
    assert_eq!(ACTION_KNOWLEDGE_SEARCH, "podcast.knowledge.search");
    assert_eq!(
        ACTION_KNOWLEDGE_CLEAR_RESULTS,
        "podcast.knowledge.clear_results"
    );
    assert_eq!(
        ACTION_KNOWLEDGE_INDEX_EPISODE,
        "podcast.knowledge.index_episode"
    );
}
#[test]
fn search_action_round_trips() {
    let action = KnowledgeAction::Search {
        query: "machine learning".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"search""#));
    assert!(json.contains(r#""query":"machine learning""#));
    let decoded: KnowledgeAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn clear_results_action_round_trips() {
    let action = KnowledgeAction::ClearResults;
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"clear_results"}"#);
    let decoded: KnowledgeAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn index_episode_action_round_trips() {
    let action = KnowledgeAction::IndexEpisode {
        episode_id: "ep-42".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"index_episode""#));
    assert!(json.contains(r#""episode_id":"ep-42""#));
    let decoded: KnowledgeAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn execute_emits_dispatch_host_op() {
    let action = KnowledgeAction::Search {
        query: "nostr".into(),
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    KnowledgeActionModule.execute(action, "corr-1", &|cmd| {
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
    assert_eq!(v["ns"], "podcast.knowledge");
    assert_eq!(v["action"]["op"], "search");
    assert_eq!(v["action"]["query"], "nostr");
}
