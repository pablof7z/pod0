//! Agent-tasks host-op routing.
//!
//! Mutates the `Arc<Mutex<Vec<AgentTaskSummary>>>` slot shared with
//! [`crate::ffi::handle::PodcastHandle`] via [`crate::ffi::actions::AgentTasksAction`]
//! dispatches. Each op bumps the supplied `rev` AtomicU64 so the next
//! snapshot poll picks up the change without an extra wake-up signal.
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

/// Seed value installed on first kernel boot — gives the iOS UI rows to
/// render before the user has scheduled anything. Returned by value so
/// `register.rs` can hand it directly to `Arc::new(Mutex::new(...))`.
pub fn default_seed() -> Vec<AgentTaskSummary> {
    let payload = task_payload_from_intent(&AgentTaskIntent::InboxTriage)
        .expect("inbox triage intent must resolve");
    vec![AgentTaskSummary {
        id: Uuid::new_v4().to_string(),
        title: "Inbox Triage".into(),
        description: Some("Surface new episodes worth your time".into()),
        action_namespace: payload.action_namespace,
        action_body: payload.action_body,
        schedule: "daily".into(),
        next_run_at: None,
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
            Ok(payload) => create_task(&mut guard, rev, title, description, payload, schedule),
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
            let Some(task) = guard.iter_mut().find(|t| t.id == task_id) else {
                return serde_json::json!({"ok": false, "error": "task not found"});
            };
            if !task.is_enabled {
                return serde_json::json!({"ok": false, "error": "task disabled"});
            }
            // Snapshot what the dispatch needs, flip to "running", stamp,
            // and bump `rev` so the next snapshot tick shows the in-flight
            // state even if there is no `dispatch` wired (unit tests).
            let action_namespace = task.action_namespace.clone();
            let action_body = task.action_body.clone();
            task.last_run_at = Some(Utc::now().timestamp());
            task.status = "running".into();
            rev.fetch_add(1, Ordering::Relaxed);

            // Release the tasks lock BEFORE re-dispatching: the production
            // `dispatch` re-enters the kernel action registry on this same
            // actor thread and we must not hold the slot across it.
            drop(guard);

            let Some(dispatch) = dispatch else {
                // No live kernel (unit tests): leave the task "running".
                return serde_json::json!({"ok": true, "status": "running"});
            };

            // Synchronous accept/reject — the dispatched action's own
            // downstream completion arrives later via the snapshot
            // projection, which this slot does not watch (see module docs).
            let accepted = dispatch(&action_namespace, &action_body);
            let status = if accepted { "completed" } else { "failed" };
            if let Ok(mut g) = tasks.lock() {
                if let Some(t) = g.iter_mut().find(|t| t.id == task_id) {
                    t.status = status.into();
                    t.last_run_at = Some(Utc::now().timestamp());
                }
            }
            rev.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({"ok": accepted, "status": status})
        }
    }
}

struct TaskPayload {
    action_namespace: String,
    action_body: String,
}

fn create_task(
    guard: &mut Vec<AgentTaskSummary>,
    rev: &Arc<AtomicU64>,
    title: String,
    description: Option<String>,
    payload: TaskPayload,
    schedule: String,
) -> serde_json::Value {
    let task_id = Uuid::new_v4().to_string();
    guard.push(AgentTaskSummary {
        id: task_id.clone(),
        title,
        description,
        action_namespace: payload.action_namespace,
        action_body: payload.action_body,
        schedule,
        next_run_at: None,
        last_run_at: None,
        status: "pending".into(),
        is_enabled: true,
    });
    rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true, "task_id": task_id})
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
