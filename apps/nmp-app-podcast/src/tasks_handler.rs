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
//! ## Run-now stub
//!
//! `run_now` does NOT actually re-dispatch the task's
//! `(action_namespace, action_body)` payload — the receiver actions
//! (`podcast.briefings.generate`, `podcast.inbox.triage`) don't exist
//! as `ActionModule`s yet, and the host-op layer can't reach back into
//! `NmpApp::dispatch_action` from inside an op handler (would deadlock
//! the actor loop). For now, `run_now` stamps `last_run_at = now()` +
//! `status = "completed"` so the UI can show the task as recently run.
//! Once the receiver actions land in a follow-up PR, the stamp can be
//! replaced with a real `ActorCommand::DispatchHostOp` enqueue without
//! changing the action wire shape.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use uuid::Uuid;

use crate::ffi::actions::AgentTasksAction;
use crate::ffi::projections::AgentTaskSummary;

/// Seed value installed on first kernel boot — gives the iOS UI rows to
/// render before the user has scheduled anything. Returned by value so
/// `register.rs` can hand it directly to `Arc::new(Mutex::new(...))`.
pub fn default_seed() -> Vec<AgentTaskSummary> {
    vec![
        AgentTaskSummary {
            id: Uuid::new_v4().to_string(),
            title: "Morning Briefing".into(),
            description: Some("Generate today's briefing".into()),
            action_namespace: "podcast.briefings.generate".into(),
            action_body: "{}".into(),
            schedule: "daily".into(),
            next_run_at: None,
            last_run_at: None,
            status: "pending".into(),
            is_enabled: true,
        },
        AgentTaskSummary {
            id: Uuid::new_v4().to_string(),
            title: "Inbox Triage".into(),
            description: Some("Surface new episodes worth your time".into()),
            action_namespace: "podcast.inbox.triage".into(),
            action_body: "{}".into(),
            schedule: "daily".into(),
            next_run_at: None,
            last_run_at: None,
            status: "pending".into(),
            is_enabled: true,
        },
    ]
}

/// Route one `podcast.tasks.*` action against the shared tasks slot.
/// Returns the JSON envelope the host-op handler forwards back to Swift.
pub fn handle_tasks_action(
    action: AgentTasksAction,
    tasks: &Arc<Mutex<Vec<AgentTaskSummary>>>,
    rev: &Arc<AtomicU64>,
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
        } => {
            let task_id = Uuid::new_v4().to_string();
            guard.push(AgentTaskSummary {
                id: task_id.clone(),
                title,
                description,
                action_namespace,
                action_body,
                schedule,
                next_run_at: None,
                last_run_at: None,
                status: "pending".into(),
                is_enabled: true,
            });
            rev.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({"ok": true, "task_id": task_id})
        }
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
            // Stub: the real dispatch lands once receiver action modules
            // (`podcast.briefings.generate`, `podcast.inbox.triage`)
            // exist. For now, mark the task as completed so the UI
            // surfaces a recent run.
            task.last_run_at = Some(Utc::now().timestamp());
            task.status = "completed".into();
            rev.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({"ok": true})
        }
    }
}

fn set_enabled(
    guard: &mut Vec<AgentTaskSummary>,
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
