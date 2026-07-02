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
fn create_action_round_trips_with_all_fields() {
    let action = AgentTasksAction::Create {
        title: "Inbox Triage".into(),
        description: Some("Daily inbox triage".into()),
        action_namespace: "podcast.inbox.triage".into(),
        action_body: "{}".into(),
        schedule: "daily".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"create""#));
    assert!(json.contains(r#""title":"Inbox Triage""#));
    assert!(json.contains(r#""action_namespace":"podcast.inbox.triage""#));
    let decoded: AgentTasksAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn create_action_omits_none_description() {
    let action = AgentTasksAction::Create {
        title: "Inbox Triage".into(),
        description: None,
        action_namespace: "podcast.inbox.triage".into(),
        action_body: "{}".into(),
        schedule: "daily".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(!json.contains("description"));
    let decoded: AgentTasksAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn create_from_intent_action_round_trips() {
    let action = AgentTasksAction::CreateFromIntent {
        title: "Clear Agent".into(),
        description: None,
        intent: AgentTaskIntent::ClearAgent,
        schedule: "once".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"create_from_intent""#));
    assert!(json.contains(r#""intent":{"type":"clear_agent"}"#));
    assert!(!json.contains("action_namespace"));
    assert!(!json.contains("action_body"));
    let decoded: AgentTasksAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn update_from_intent_action_round_trips() {
    let action = AgentTasksAction::UpdateFromIntent {
        task_id: "task-1".into(),
        title: "Prompt".into(),
        description: Some("Daily prompt".into()),
        intent: AgentTaskIntent::AgentPrompt {
            prompt: "summarize new episodes".into(),
        },
        schedule: "daily".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"update_from_intent""#));
    assert!(json.contains(r#""intent":{"type":"agent_prompt""#));
    assert!(!json.contains("action_namespace"));
    assert!(!json.contains("action_body"));
    let decoded: AgentTasksAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn memory_intent_round_trips() {
    let intent = AgentTaskIntent::RememberMemory {
        key: "focus".into(),
        value: "podcasts".into(),
    };
    let json = serde_json::to_string(&intent).expect("encode");
    assert_eq!(
        json,
        r#"{"type":"remember_memory","key":"focus","value":"podcasts"}"#
    );
    let decoded: AgentTaskIntent = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, intent);
}

#[test]
fn delete_action_round_trips() {
    let action = AgentTasksAction::Delete {
        task_id: "task-1".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"delete""#));
    assert!(json.contains(r#""task_id":"task-1""#));
    let decoded: AgentTasksAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn enable_disable_actions_round_trip() {
    for (action, expected_op) in [
        (
            AgentTasksAction::Enable {
                task_id: "task-1".into(),
            },
            "enable",
        ),
        (
            AgentTasksAction::Disable {
                task_id: "task-1".into(),
            },
            "disable",
        ),
    ] {
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(&format!(r#""op":"{expected_op}""#)));
        let decoded: AgentTasksAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }
}
#[test]
fn run_now_action_round_trips() {
    let action = AgentTasksAction::RunNow {
        task_id: "task-1".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"run_now""#));
    let decoded: AgentTasksAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn run_due_action_round_trips() {
    let action = AgentTasksAction::RunDue;
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"run_due"}"#);
    let decoded: AgentTasksAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}
#[test]
fn execute_emits_dispatch_host_op() {
    let action = AgentTasksAction::Delete {
        task_id: "task-1".into(),
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    AgentTasksModule
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
    assert_eq!(v["ns"], "podcast.tasks");
    assert_eq!(v["action"]["op"], "delete");
    assert_eq!(v["action"]["task_id"], "task-1");
}
