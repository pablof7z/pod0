//! Tests for [`super::tasks_handler`] — AgentTasksHandler create/delete/enable/disable/run.
//!
//! Extracted from `tasks_handler.rs` to keep that file under the 500-line hard limit.

use super::*;
use nmp_core::substrate::ActionModule;

fn new_state() -> (Arc<Mutex<Vec<AgentTaskSummary>>>, Arc<AtomicU64>) {
    (
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(AtomicU64::new(0)),
    )
}

const TEST_NOW: i64 = 1_700_000_000_i64;

#[test]
fn default_seed_has_inbox_triage_task() {
    let seed = default_seed(TEST_NOW);
    assert_eq!(seed.len(), 1);
    assert_eq!(seed[0].title, "Inbox Triage");
    assert_eq!(seed[0].intent_type, "inbox_triage");
    assert_eq!(seed[0].intent_label, "Triage inbox");
    // Seed namespaces MUST match a *registered* `ActionModule::NAMESPACE`
    // (exact `modules.get(namespace)` lookup) so `RunNow` actually
    // dispatches. Bind to the real consts so future drift fails loudly.
    assert_eq!(
        seed[0].action_namespace,
        crate::ffi::actions::InboxActionModule::NAMESPACE.as_str()
    );
    assert_eq!(seed[0].action_body, r#"{"op":"triage"}"#);
    assert!(seed.iter().all(|t| t.is_enabled));
    assert!(seed.iter().all(|t| t.status == "pending"));
    // next_run_at must be daily (86400 s) from the injected timestamp.
    assert_eq!(seed[0].next_run_at, Some(TEST_NOW + 86_400));
    // Id must be a hyphenated UUID.
    assert!(Uuid::parse_str(&seed[0].id).is_ok());
}

#[test]
fn create_appends_and_returns_task_id() {
    let (tasks, rev) = new_state();
    let result = handle_tasks_action(
        AgentTasksAction::CreateFromIntent {
            title: "Triage".into(),
            description: None,
            intent: AgentTaskIntent::InboxTriage,
            schedule: "daily".into(),
        },
        &tasks,
        &rev,
        None,
    );
    assert_eq!(result["ok"], true);
    let task_id = result["task_id"].as_str().expect("task_id present");
    assert!(Uuid::parse_str(task_id).is_ok());
    let guard = tasks.lock().unwrap();
    assert_eq!(guard.len(), 1);
    assert_eq!(guard[0].title, "Triage");
    assert_eq!(guard[0].id, task_id);
    assert_eq!(guard[0].intent_type, "inbox_triage");
    assert_eq!(guard[0].intent_label, "Triage inbox");
    assert_eq!(guard[0].action_namespace, "podcast.inbox");
    assert_eq!(guard[0].action_body, r#"{"op":"triage"}"#);
    assert!(guard[0].next_run_at.is_some());
    assert_eq!(rev.load(Ordering::Relaxed), 1);
}

#[test]
fn create_from_agent_prompt_intent_resolves_agent_send_action() {
    let (tasks, rev) = new_state();
    let result = handle_tasks_action(
        AgentTasksAction::CreateFromIntent {
            title: "Daily prompt".into(),
            description: None,
            intent: AgentTaskIntent::AgentPrompt {
                prompt: "summarize my new episodes".into(),
            },
            schedule: "every 60s".into(),
        },
        &tasks,
        &rev,
        None,
    );
    assert_eq!(result["ok"], true);
    let guard = tasks.lock().unwrap();
    assert_eq!(guard[0].intent_type, "agent_prompt");
    assert_eq!(guard[0].intent_label, "Agent prompt");
    assert_eq!(
        guard[0].intent_detail.as_deref(),
        Some("summarize my new episodes")
    );
    assert_eq!(guard[0].action_namespace, "podcast.agent");
    assert_eq!(
        guard[0].action_body,
        r#"{"op":"send","message":"summarize my new episodes"}"#
    );
}

#[test]
fn create_rejects_invalid_schedule_without_bumping_rev() {
    let (tasks, rev) = new_state();
    let result = handle_tasks_action(
        AgentTasksAction::CreateFromIntent {
            title: "Bad".into(),
            description: None,
            intent: AgentTaskIntent::InboxTriage,
            schedule: "whenever".into(),
        },
        &tasks,
        &rev,
        None,
    );
    assert_eq!(result["ok"], false);
    assert!(result["error"]
        .as_str()
        .unwrap()
        .contains("invalid schedule"));
    assert!(tasks.lock().unwrap().is_empty());
    assert_eq!(rev.load(Ordering::Relaxed), 0);
}

#[test]
fn create_raw_payload_still_appends_for_compatibility() {
    let (tasks, rev) = new_state();
    let result = handle_tasks_action(
        AgentTasksAction::Create {
            title: "Research X".into(),
            description: None,
            action_namespace: "podcast.research".into(),
            action_body: "{\"topic\":\"x\"}".into(),
            schedule: "once".into(),
        },
        &tasks,
        &rev,
        None,
    );
    assert_eq!(result["ok"], true);
    let guard = tasks.lock().unwrap();
    assert_eq!(guard[0].intent_type, "custom");
    assert_eq!(guard[0].intent_label, "Custom task");
    assert_eq!(guard[0].action_namespace, "podcast.research");
    assert_eq!(guard[0].action_body, "{\"topic\":\"x\"}");
    assert_eq!(rev.load(Ordering::Relaxed), 1);
}

#[test]
fn create_from_memory_intent_resolves_memory_action() {
    let (tasks, rev) = new_state();
    let result = handle_tasks_action(
        AgentTasksAction::CreateFromIntent {
            title: "Remember Preference".into(),
            description: Some("keep preference fresh".into()),
            intent: AgentTaskIntent::RememberMemory {
                key: "topic".into(),
                value: "rust".into(),
            },
            schedule: "weekly".into(),
        },
        &tasks,
        &rev,
        None,
    );
    assert_eq!(result["ok"], true);
    let guard = tasks.lock().unwrap();
    assert_eq!(guard[0].intent_type, "remember_memory");
    assert_eq!(guard[0].intent_label, "Remember memory");
    assert_eq!(guard[0].intent_detail.as_deref(), Some("topic = rust"));
    assert_eq!(guard[0].action_namespace, "podcast.memory");
    assert_eq!(
        guard[0].action_body,
        r#"{"op":"remember","key":"topic","value":"rust","source":"task"}"#
    );
}

#[test]
fn update_from_intent_replaces_task_payload_and_schedule() {
    let (tasks, rev) = new_state();
    let create = handle_tasks_action(
        AgentTasksAction::CreateFromIntent {
            title: "Triage".into(),
            description: None,
            intent: AgentTaskIntent::InboxTriage,
            schedule: "daily".into(),
        },
        &tasks,
        &rev,
        None,
    );
    let task_id = create["task_id"].as_str().unwrap().to_owned();
    let update = handle_tasks_action(
        AgentTasksAction::UpdateFromIntent {
            task_id,
            title: "Remember".into(),
            description: Some("new task".into()),
            intent: AgentTaskIntent::RememberMemory {
                key: "topic".into(),
                value: "rust".into(),
            },
            schedule: "weekly".into(),
        },
        &tasks,
        &rev,
        None,
    );
    assert_eq!(update["ok"], true);
    let guard = tasks.lock().unwrap();
    assert_eq!(guard[0].title, "Remember");
    assert_eq!(guard[0].description.as_deref(), Some("new task"));
    assert_eq!(guard[0].intent_type, "remember_memory");
    assert_eq!(guard[0].schedule, "weekly");
    assert_eq!(guard[0].status, "pending");
    assert!(guard[0].next_run_at.is_some());
}

#[test]
fn delete_removes_known_task_and_bumps_rev() {
    let (tasks, rev) = new_state();
    let create = handle_tasks_action(
        AgentTasksAction::Create {
            title: "Tmp".into(),
            description: None,
            action_namespace: "podcast.research".into(),
            action_body: "{}".into(),
            schedule: "once".into(),
        },
        &tasks,
        &rev,
        None,
    );
    let task_id = create["task_id"].as_str().unwrap().to_string();
    let before_rev = rev.load(Ordering::Relaxed);
    let del = handle_tasks_action(
        AgentTasksAction::Delete {
            task_id: task_id.clone(),
        },
        &tasks,
        &rev,
        None,
    );
    assert_eq!(del["ok"], true);
    assert!(tasks.lock().unwrap().is_empty());
    assert_eq!(rev.load(Ordering::Relaxed), before_rev + 1);
}

#[test]
fn delete_unknown_task_reports_error_without_bumping_rev() {
    let (tasks, rev) = new_state();
    let before_rev = rev.load(Ordering::Relaxed);
    let del = handle_tasks_action(
        AgentTasksAction::Delete {
            task_id: "missing".into(),
        },
        &tasks,
        &rev,
        None,
    );
    assert_eq!(del["ok"], false);
    assert_eq!(rev.load(Ordering::Relaxed), before_rev);
}

#[test]
fn enable_disable_flip_flag_and_bump_rev_only_on_change() {
    let (tasks, rev) = new_state();
    let create = handle_tasks_action(
        AgentTasksAction::Create {
            title: "T".into(),
            description: None,
            action_namespace: "podcast.x".into(),
            action_body: "{}".into(),
            schedule: "daily".into(),
        },
        &tasks,
        &rev,
        None,
    );
    let task_id = create["task_id"].as_str().unwrap().to_string();
    let rev_after_create = rev.load(Ordering::Relaxed);

    // Disable flips false → rev bumps.
    let disable = handle_tasks_action(
        AgentTasksAction::Disable {
            task_id: task_id.clone(),
        },
        &tasks,
        &rev,
        None,
    );
    assert_eq!(disable["ok"], true);
    assert!(!tasks.lock().unwrap()[0].is_enabled);
    assert_eq!(rev.load(Ordering::Relaxed), rev_after_create + 1);

    // Disable again is a no-op → rev unchanged.
    let _ = handle_tasks_action(
        AgentTasksAction::Disable {
            task_id: task_id.clone(),
        },
        &tasks,
        &rev,
        None,
    );
    assert_eq!(rev.load(Ordering::Relaxed), rev_after_create + 1);

    // Enable flips back → rev bumps.
    let _ = handle_tasks_action(
        AgentTasksAction::Enable {
            task_id: task_id.clone(),
        },
        &tasks,
        &rev,
        None,
    );
    assert!(tasks.lock().unwrap()[0].is_enabled);
    assert_eq!(rev.load(Ordering::Relaxed), rev_after_create + 2);
}

/// Helper: create one enabled task and return its id.
fn create_task(tasks: &Arc<Mutex<Vec<AgentTaskSummary>>>, rev: &Arc<AtomicU64>) -> String {
    let create = handle_tasks_action(
        AgentTasksAction::Create {
            title: "T".into(),
            description: None,
            action_namespace: "podcast.x".into(),
            action_body: "{}".into(),
            schedule: "once".into(),
        },
        tasks,
        rev,
        None,
    );
    create["task_id"].as_str().unwrap().to_string()
}

#[test]
fn run_now_without_dispatch_stamps_running() {
    // No live kernel (dispatch = None): the task flips to "running" and
    // stamps `last_run_at`, but stays there (no synchronous accept/reject).
    let (tasks, rev) = new_state();
    let task_id = create_task(&tasks, &rev);
    let result = handle_tasks_action(
        AgentTasksAction::RunNow {
            task_id: task_id.clone(),
        },
        &tasks,
        &rev,
        None,
    );
    assert_eq!(result["ok"], true);
    assert_eq!(result["status"], "running");
    let guard = tasks.lock().unwrap();
    assert_eq!(guard[0].status, "running");
    assert!(guard[0].last_run_at.is_some());
}

#[test]
fn run_due_runs_due_task_and_clears_once_next_run() {
    let (tasks, rev) = new_state();
    let create = handle_tasks_action(
        AgentTasksAction::CreateFromIntent {
            title: "Clear".into(),
            description: None,
            intent: AgentTaskIntent::ClearAgent,
            schedule: "once".into(),
        },
        &tasks,
        &rev,
        None,
    );
    let task_id = create["task_id"].as_str().unwrap().to_owned();
    let dispatch = |_ns: &str, _body: &str| true;
    let result = handle_tasks_action(AgentTasksAction::RunDue, &tasks, &rev, Some(&dispatch));
    assert_eq!(result["ok"], true);
    assert_eq!(result["ran"], 1);
    assert_eq!(result["accepted"], 1); // dispatch was accepted (task is in-flight)
    let guard = tasks.lock().unwrap();
    assert_eq!(guard[0].id, task_id);
    // Dispatch accepted → in-flight, never "completed" prematurely.
    assert_eq!(guard[0].status, "running");
    assert!(guard[0].last_run_at.is_some());
    assert_eq!(guard[0].next_run_at, None);
}

#[test]
fn run_now_marks_failed_on_reject() {
    // A rejecting dispatch (unknown namespace / bad body) → "failed".
    let (tasks, rev) = new_state();
    let task_id = create_task(&tasks, &rev);
    let dispatch = |_ns: &str, _body: &str| false;
    let result = handle_tasks_action(
        AgentTasksAction::RunNow {
            task_id: task_id.clone(),
        },
        &tasks,
        &rev,
        Some(&dispatch),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["status"], "failed");
    let guard = tasks.lock().unwrap();
    assert_eq!(guard[0].status, "failed");
}

#[test]
fn run_now_forwards_namespace_and_body_to_dispatch() {
    // The seeded (namespace, body) pair is what reaches the dispatch hook —
    // the contract `RunNow` re-dispatches.
    let seed = default_seed(TEST_NOW);
    let tasks = Arc::new(Mutex::new(seed.clone()));
    let rev = Arc::new(AtomicU64::new(0));
    let captured: std::sync::Mutex<Option<(String, String)>> = std::sync::Mutex::new(None);
    let dispatch = |ns: &str, body: &str| {
        *captured.lock().unwrap() = Some((ns.to_owned(), body.to_owned()));
        true
    };
    let _ = handle_tasks_action(
        AgentTasksAction::RunNow {
            task_id: seed[0].id.clone(), // Inbox Triage
        },
        &tasks,
        &rev,
        Some(&dispatch),
    );
    let got = captured.lock().unwrap().clone().expect("dispatch called");
    assert_eq!(got.0, "podcast.inbox");
    assert_eq!(got.1, r#"{"op":"triage"}"#);
}

#[test]
fn run_now_disabled_task_fails() {
    let (tasks, rev) = new_state();
    let task_id = create_task(&tasks, &rev);
    // Disable, then RunNow should refuse without dispatching.
    let _ = handle_tasks_action(
        AgentTasksAction::Disable {
            task_id: task_id.clone(),
        },
        &tasks,
        &rev,
        None,
    );
    let dispatched = std::sync::atomic::AtomicBool::new(false);
    let dispatch = |_ns: &str, _body: &str| {
        dispatched.store(true, Ordering::Relaxed);
        true
    };
    let result = handle_tasks_action(
        AgentTasksAction::RunNow {
            task_id: task_id.clone(),
        },
        &tasks,
        &rev,
        Some(&dispatch),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"], "task disabled");
    assert!(
        !dispatched.load(Ordering::Relaxed),
        "disabled task must not dispatch"
    );
}

#[test]
fn run_now_unknown_task_reports_error() {
    let (tasks, rev) = new_state();
    let result = handle_tasks_action(
        AgentTasksAction::RunNow {
            task_id: "missing".into(),
        },
        &tasks,
        &rev,
        None,
    );
    assert_eq!(result["ok"], false);
}
