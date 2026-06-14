//! Agent-tasks host-op routing.
//!
//! Mutates the `Arc<Mutex<Vec<AgentTaskSummary>>>` slot shared with
//! [`crate::ffi::handle::PodcastHandle`] via [`crate::ffi::actions::AgentTasksAction`]
//! dispatches. Each op bumps the supplied `rev` AtomicU64 so the next
//! snapshot frame picks up the change without an extra wake-up signal.
//!
//! Pulled into its own module so `host_op_handler.rs` stays under the
//! 500-line hard limit (it was at 499 before the M14 task ops landed).
//!
//! ## Run-now dispatch
//!
//! `run_now` re-dispatches the task's stored `(action_namespace, action_body)`
//! payload through the kernel action registry via the `dispatch`
//! callback the call site injects (production wraps
//! `nmp_ffi::nmp_app_dispatch_action`). The callback runs *synchronously*
//! on the actor thread — `nmp_app_dispatch_action` only validates the
//! action and enqueues an `ActorCommand::DispatchHostOp` (D8: no actor
//! round-trip on the FFI thread), so re-entry from inside a host-op
//! handler appends to the actor's own queue and returns immediately —
//! there is no deadlock and nothing crosses a thread boundary. This
//! mirrors the existing synchronous `dispatch_capability` precedent in
//! `host_op_handler.rs`.
//!
//! Status mapping (synchronous accept/reject is all `run_now` can
//! observe — the dispatched action's *downstream* completion arrives
//! later via the snapshot projection, which `agent_tasks` does not
//! watch, so "completed" here means "successfully dispatched/accepted",
//! not "downstream work finished"):
//!
//! * accepted (the registry minted a `correlation_id`) → `"completed"`
//! * rejected (unknown namespace / bad body) → `"failed"`
//!
//! ## Namespace contract
//!
//! `create_from_intent` is the user-facing creation path: it resolves a typed
//! [`AgentTaskIntent`] to an internal dispatch payload here, so clients do not
//! have to know or edit action namespace/body JSON. The legacy `create` op
//! still accepts raw payloads for compatibility.
//!
//! `action_namespace` must be a *registered* `ActionModule::NAMESPACE`
//! (the registry does an exact `modules.get(namespace)` lookup — there
//! is no prefix routing) and `action_body` must be that module's
//! `{"op":...}` wire body. The default seed (see [`default_seed`]) uses
//! the real namespaces:
//!
//! * Inbox Triage → ns `"podcast.inbox"`, body `{"op":"triage"}`

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use nmp_core::substrate::ActionModule;
use uuid::Uuid;

use crate::ffi::actions::{
    AgentActionModule, AgentChatAction, AgentTaskIntent, AgentTasksAction, InboxAction,
    InboxActionModule, MemoryAction, MemoryActionModule,
};
use crate::ffi::projections::AgentTaskSummary;
use crate::tasks_schedule::{next_run_after, next_run_after_attempt};

/// Seed value installed on first kernel boot — gives the iOS UI rows to
/// render before the user has scheduled anything. Returned by value so
/// `register.rs` can hand it directly to `Arc::new(Mutex::new(...))`.
pub fn default_seed() -> Vec<AgentTaskSummary> {
    let intent = AgentTaskIntent::InboxTriage;
    let payload = task_payload_from_intent(&intent).expect("inbox triage intent must resolve");
    let metadata = task_intent_metadata(Some(&intent));
    vec![AgentTaskSummary {
        id: Uuid::new_v4().to_string(),
        title: "Inbox Triage".into(),
        description: Some("Surface new episodes worth your time".into()),
        intent_type: metadata.intent_type,
        intent_label: metadata.intent_label,
        intent_detail: metadata.intent_detail,
        action_namespace: payload.action_namespace,
        action_body: payload.action_body,
        schedule: "daily".into(),
        next_run_at: next_run_after("daily", Utc::now().timestamp())
            .ok()
            .flatten(),
        last_run_at: None,
        status: "pending".into(),
        is_enabled: true,
    }]
}

/// Synchronous action-dispatch callback injected by the call site.
///
/// Called as `dispatch(action_namespace, action_body)` and returns `true`
/// when the dispatch was *accepted* by the kernel action registry (a
/// `correlation_id` was minted) or `false` when it was *rejected*
/// (unknown namespace / malformed body). Production wraps
/// `nmp_ffi::nmp_app_dispatch_action` (and frees the returned C string);
/// tests inject a deterministic closure.
///
/// Kept as a borrowed trait object so the raw `*mut NmpApp` never leaves
/// `host_op_handler.rs` — `handle_tasks_action` stays `app`-free and
/// unit-testable without a live kernel.
pub type TaskDispatchFn<'a> = dyn Fn(&str, &str) -> bool + 'a;

/// Optional persistence callback for callers that have a bound data directory.
pub type TaskPersistFn<'a> = dyn Fn(&[AgentTaskSummary]) + 'a;

/// Route one task action and persist the final task projection when it changes.
pub fn handle_tasks_action_with_persist(
    action: AgentTasksAction,
    tasks: &Arc<Mutex<Vec<AgentTaskSummary>>>,
    rev: &Arc<AtomicU64>,
    dispatch: Option<&TaskDispatchFn<'_>>,
    persist: Option<&TaskPersistFn<'_>>,
) -> serde_json::Value {
    let before = match tasks.lock() {
        Ok(guard) => Some(guard.clone()),
        Err(_) => None,
    };
    let result = handle_tasks_action(action, tasks, rev, dispatch);
    let Some(persist) = persist else {
        return result;
    };
    let Ok(guard) = tasks.lock() else {
        return result;
    };
    let snapshot = guard.clone();
    drop(guard);
    let changed = match before {
        Some(before) => before != snapshot,
        None => true,
    };
    if changed {
        persist(&snapshot);
    }
    result
}

/// Route one `podcast.tasks.*` action against the shared tasks slot.
/// Returns the JSON envelope the host-op handler forwards back to Swift.
///
/// `dispatch` is the synchronous re-dispatch hook used by `RunNow`
/// (see [`TaskDispatchFn`]); `None` skips the dispatch (unit tests with
/// no live kernel) and leaves the task in `"running"`.
pub fn handle_tasks_action(
    action: AgentTasksAction,
    tasks: &Arc<Mutex<Vec<AgentTaskSummary>>>,
    rev: &Arc<AtomicU64>,
    dispatch: Option<&TaskDispatchFn<'_>>,
) -> serde_json::Value {
    let Ok(mut guard) = tasks.lock() else {
        return serde_json::json!({"ok": false, "error": "tasks slot poisoned"});
    };
    match action {
        AgentTasksAction::Create {
            title,
            description,
            action_namespace,
            action_body,
            schedule,
        } => create_task(
            &mut guard,
            rev,
            title,
            description,
            None,
            TaskPayload {
                action_namespace,
                action_body,
            },
            schedule,
        ),
        AgentTasksAction::CreateFromIntent {
            title,
            description,
            intent,
            schedule,
        } => match task_payload_from_intent(&intent) {
            Ok(payload) => create_task(
                &mut guard,
                rev,
                title,
                description,
                Some(intent),
                payload,
                schedule,
            ),
            Err(error) => serde_json::json!({"ok": false, "error": error}),
        },
        AgentTasksAction::UpdateFromIntent {
            task_id,
            title,
            description,
            intent,
            schedule,
        } => match task_payload_from_intent(&intent) {
            Ok(payload) => update_task(
                &mut guard,
                rev,
                task_id,
                title,
                description,
                intent,
                payload,
                schedule,
            ),
            Err(error) => serde_json::json!({"ok": false, "error": error}),
        },
        AgentTasksAction::Delete { task_id } => {
            let before = guard.len();
            guard.retain(|t| t.id != task_id);
            if guard.len() == before {
                serde_json::json!({"ok": false, "error": "task not found"})
            } else {
                rev.fetch_add(1, Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
        }
        AgentTasksAction::Enable { task_id } => set_enabled(&mut guard, &task_id, true, rev),
        AgentTasksAction::Disable { task_id } => set_enabled(&mut guard, &task_id, false, rev),
        AgentTasksAction::RunNow { task_id } => {
            drop(guard);
            run_task_by_id(tasks, rev, &task_id, dispatch, Utc::now().timestamp())
        }
        AgentTasksAction::RunDue => {
            let now = Utc::now().timestamp();
            let task_ids = guard
                .iter()
                .filter(|task| task.is_enabled && task.next_run_at.is_some_and(|due| due <= now))
                .map(|task| task.id.clone())
                .collect::<Vec<_>>();
            drop(guard);

            let mut accepted = 0;
            let mut failed = 0;
            let mut running = 0;
            for task_id in &task_ids {
                let result = run_task_by_id(tasks, rev, task_id, dispatch, now);
                match result["status"].as_str() {
                    Some("completed") => accepted += 1,
                    Some("failed") => failed += 1,
                    Some("running") => running += 1,
                    _ => {}
                }
            }
            serde_json::json!({
                "ok": failed == 0,
                "ran": task_ids.len(),
                "accepted": accepted,
                "failed": failed,
                "running": running,
            })
        }
    }
}

struct TaskPayload {
    action_namespace: String,
    action_body: String,
}

struct TaskIntentMetadata {
    intent_type: String,
    intent_label: String,
    intent_detail: Option<String>,
}

fn create_task(
    guard: &mut Vec<AgentTaskSummary>,
    rev: &Arc<AtomicU64>,
    title: String,
    description: Option<String>,
    intent: Option<AgentTaskIntent>,
    payload: TaskPayload,
    schedule: String,
) -> serde_json::Value {
    let now = Utc::now().timestamp();
    let next_run_at = match next_run_after(&schedule, now) {
        Ok(next) => next,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let task_id = Uuid::new_v4().to_string();
    let metadata = task_intent_metadata(intent.as_ref());
    guard.push(AgentTaskSummary {
        id: task_id.clone(),
        title,
        description,
        intent_type: metadata.intent_type,
        intent_label: metadata.intent_label,
        intent_detail: metadata.intent_detail,
        action_namespace: payload.action_namespace,
        action_body: payload.action_body,
        schedule,
        next_run_at,
        last_run_at: None,
        status: "pending".into(),
        is_enabled: true,
    });
    rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true, "task_id": task_id})
}

fn task_intent_metadata(intent: Option<&AgentTaskIntent>) -> TaskIntentMetadata {
    match intent {
        Some(AgentTaskIntent::InboxTriage) => TaskIntentMetadata {
            intent_type: "inbox_triage".to_owned(),
            intent_label: "Triage inbox".to_owned(),
            intent_detail: Some("Prioritize new episodes".to_owned()),
        },
        Some(AgentTaskIntent::ClearAgent) => TaskIntentMetadata {
            intent_type: "clear_agent".to_owned(),
            intent_label: "Clear agent chat".to_owned(),
            intent_detail: None,
        },
        Some(AgentTaskIntent::RememberMemory { key, value }) => TaskIntentMetadata {
            intent_type: "remember_memory".to_owned(),
            intent_label: "Remember memory".to_owned(),
            intent_detail: Some(format!("{key} = {value}")),
        },
        Some(AgentTaskIntent::AgentPrompt { prompt }) => TaskIntentMetadata {
            intent_type: "agent_prompt".to_owned(),
            intent_label: "Agent prompt".to_owned(),
            intent_detail: Some(prompt.clone()),
        },
        None => TaskIntentMetadata {
            intent_type: "custom".to_owned(),
            intent_label: "Custom task".to_owned(),
            intent_detail: None,
        },
    }
}

fn task_payload_from_intent(intent: &AgentTaskIntent) -> Result<TaskPayload, String> {
    match intent {
        AgentTaskIntent::InboxTriage => task_payload(
            <InboxActionModule as ActionModule>::NAMESPACE,
            &InboxAction::Triage,
        ),
        AgentTaskIntent::ClearAgent => task_payload(
            <AgentActionModule as ActionModule>::NAMESPACE,
            &AgentChatAction::Clear,
        ),
        AgentTaskIntent::RememberMemory { key, value } => task_payload(
            <MemoryActionModule as ActionModule>::NAMESPACE,
            &MemoryAction::Remember {
                key: key.clone(),
                value: value.clone(),
                source: Some("task".into()),
            },
        ),
        AgentTaskIntent::AgentPrompt { prompt } => task_payload(
            <AgentActionModule as ActionModule>::NAMESPACE,
            &AgentChatAction::Send {
                message: prompt.clone(),
            },
        ),
    }
}

fn task_payload<T: serde::Serialize>(
    action_namespace: &str,
    action: &T,
) -> Result<TaskPayload, String> {
    Ok(TaskPayload {
        action_namespace: action_namespace.to_owned(),
        action_body: serde_json::to_string(action)
            .map_err(|e| format!("failed to encode task intent action: {e}"))?,
    })
}

fn update_task(
    guard: &mut [AgentTaskSummary],
    rev: &Arc<AtomicU64>,
    task_id: String,
    title: String,
    description: Option<String>,
    intent: AgentTaskIntent,
    payload: TaskPayload,
    schedule: String,
) -> serde_json::Value {
    let Some(task) = guard.iter_mut().find(|t| t.id == task_id) else {
        return serde_json::json!({"ok": false, "error": "task not found"});
    };
    let now = Utc::now().timestamp();
    let next_run_at = match next_run_after(&schedule, now) {
        Ok(next) => next,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let metadata = task_intent_metadata(Some(&intent));
    task.title = title;
    task.description = description;
    task.intent_type = metadata.intent_type;
    task.intent_label = metadata.intent_label;
    task.intent_detail = metadata.intent_detail;
    task.action_namespace = payload.action_namespace;
    task.action_body = payload.action_body;
    task.schedule = schedule;
    task.next_run_at = next_run_at;
    task.status = "pending".into();
    task.is_enabled = true;
    rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true})
}

fn run_task_by_id(
    tasks: &Arc<Mutex<Vec<AgentTaskSummary>>>,
    rev: &Arc<AtomicU64>,
    task_id: &str,
    dispatch: Option<&TaskDispatchFn<'_>>,
    now: i64,
) -> serde_json::Value {
    let Ok(mut guard) = tasks.lock() else {
        return serde_json::json!({"ok": false, "error": "tasks slot poisoned"});
    };
    let Some(task) = guard.iter_mut().find(|t| t.id == task_id) else {
        return serde_json::json!({"ok": false, "error": "task not found"});
    };
    if !task.is_enabled {
        return serde_json::json!({"ok": false, "error": "task disabled"});
    }
    let action_namespace = task.action_namespace.clone();
    let action_body = task.action_body.clone();
    let schedule = task.schedule.clone();
    task.last_run_at = Some(now);
    task.status = "running".into();
    rev.fetch_add(1, Ordering::Relaxed);
    drop(guard);

    let Some(dispatch) = dispatch else {
        return serde_json::json!({"ok": true, "status": "running"});
    };

    let accepted = dispatch(&action_namespace, &action_body);
    let status = if accepted { "completed" } else { "failed" };
    if let Ok(mut g) = tasks.lock() {
        if let Some(t) = g.iter_mut().find(|t| t.id == task_id) {
            t.status = status.into();
            t.last_run_at = Some(now);
            t.next_run_at = next_run_after_attempt(&schedule, now).ok().flatten();
        }
    }
    rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": accepted, "status": status})
}

fn set_enabled(
    guard: &mut [AgentTaskSummary],
    task_id: &str,
    enabled: bool,
    rev: &Arc<AtomicU64>,
) -> serde_json::Value {
    let Some(task) = guard.iter_mut().find(|t| t.id == task_id) else {
        return serde_json::json!({"ok": false, "error": "task not found"});
    };
    if task.is_enabled != enabled {
        task.is_enabled = enabled;
        rev.fetch_add(1, Ordering::Relaxed);
    }
    serde_json::json!({"ok": true})
}

#[cfg(test)]
#[path = "tasks_handler_tests.rs"]
mod tests;
