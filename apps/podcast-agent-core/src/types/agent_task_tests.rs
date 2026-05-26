use super::*;


#[test]
fn task_kind_round_trips() {
    let cases = [
        TaskKind::Transcribe {
            episode_id: "ep-1".into(),
        },
        TaskKind::Summarize {
            episode_id: "ep-1".into(),
        },
        TaskKind::Search {
            query: "rust async".into(),
        },
        TaskKind::Compose {
            topic: "weekly".into(),
        },
    ];
    for k in cases {
        let j = serde_json::to_string(&k).expect("encode");
        let d: TaskKind = serde_json::from_str(&j).expect("decode");
        assert_eq!(d, k);
    }
}

#[test]
fn task_kind_uses_snake_case_tag() {
    let k = TaskKind::Transcribe {
        episode_id: "ep-1".into(),
    };
    let j = serde_json::to_string(&k).expect("encode");
    assert_eq!(j, r#"{"kind":"transcribe","episode_id":"ep-1"}"#);
}

#[test]
fn task_status_round_trips() {
    let cases = [
        TaskStatus::Pending,
        TaskStatus::Running,
        TaskStatus::Completed,
        TaskStatus::Failed {
            error: "boom".into(),
        },
    ];
    for s in cases {
        let j = serde_json::to_string(&s).expect("encode");
        let d: TaskStatus = serde_json::from_str(&j).expect("decode");
        assert_eq!(d, s);
    }
}

#[test]
fn task_status_terminality() {
    assert!(!TaskStatus::Pending.is_terminal());
    assert!(!TaskStatus::Running.is_terminal());
    assert!(TaskStatus::Completed.is_terminal());
    assert!(TaskStatus::Failed {
        error: "x".into()
    }
    .is_terminal());
}

#[test]
fn agent_task_round_trips() {
    let t = AgentTask {
        id: Uuid::nil(),
        kind: TaskKind::Search {
            query: "q".into(),
        },
        status: TaskStatus::Pending,
        created_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
    };
    let j = serde_json::to_string(&t).expect("encode");
    let d: AgentTask = serde_json::from_str(&j).expect("decode");
    assert_eq!(d, t);
}
