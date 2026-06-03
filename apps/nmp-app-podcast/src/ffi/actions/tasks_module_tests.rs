use super::*;
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
fn execute_emits_dispatch_host_op() {
    let action = AgentTasksAction::Delete {
        task_id: "task-1".into(),
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    AgentTasksModule::execute(action, "corr-1", &|cmd| {
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
    assert_eq!(v["op"], "delete");
    assert_eq!(v["task_id"], "task-1");
}

