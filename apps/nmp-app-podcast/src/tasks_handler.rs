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
mod tests {
    use super::*;

    fn new_state() -> (Arc<Mutex<Vec<AgentTaskSummary>>>, Arc<AtomicU64>) {
        (Arc::new(Mutex::new(Vec::new())), Arc::new(AtomicU64::new(0)))
    }

    #[test]
    fn default_seed_has_two_default_tasks() {
        let seed = default_seed();
        assert_eq!(seed.len(), 2);
        assert_eq!(seed[0].title, "Morning Briefing");
        assert_eq!(seed[0].action_namespace, "podcast.briefings.generate");
        assert_eq!(seed[1].title, "Inbox Triage");
        assert_eq!(seed[1].action_namespace, "podcast.inbox.triage");
        assert!(seed.iter().all(|t| t.is_enabled));
        assert!(seed.iter().all(|t| t.status == "pending"));
        // Ids must be unique hyphenated UUIDs.
        assert_ne!(seed[0].id, seed[1].id);
        assert!(Uuid::parse_str(&seed[0].id).is_ok());
    }

    #[test]
    fn create_appends_and_returns_task_id() {
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
        );
        assert_eq!(result["ok"], true);
        let task_id = result["task_id"].as_str().expect("task_id present");
        assert!(Uuid::parse_str(task_id).is_ok());
        let guard = tasks.lock().unwrap();
        assert_eq!(guard.len(), 1);
        assert_eq!(guard[0].title, "Research X");
        assert_eq!(guard[0].id, task_id);
        assert_eq!(rev.load(Ordering::Relaxed), 1);
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
        );
        let task_id = create["task_id"].as_str().unwrap().to_string();
        let before_rev = rev.load(Ordering::Relaxed);
        let del = handle_tasks_action(
            AgentTasksAction::Delete {
                task_id: task_id.clone(),
            },
            &tasks,
            &rev,
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
        );
        assert_eq!(rev.load(Ordering::Relaxed), rev_after_create + 1);

        // Enable flips back → rev bumps.
        let _ = handle_tasks_action(
            AgentTasksAction::Enable {
                task_id: task_id.clone(),
            },
            &tasks,
            &rev,
        );
        assert!(tasks.lock().unwrap()[0].is_enabled);
        assert_eq!(rev.load(Ordering::Relaxed), rev_after_create + 2);
    }

    #[test]
    fn run_now_stamps_last_run_and_sets_completed() {
        let (tasks, rev) = new_state();
        let create = handle_tasks_action(
            AgentTasksAction::Create {
                title: "T".into(),
                description: None,
                action_namespace: "podcast.x".into(),
                action_body: "{}".into(),
                schedule: "once".into(),
            },
            &tasks,
            &rev,
        );
        let task_id = create["task_id"].as_str().unwrap().to_string();
        let result = handle_tasks_action(
            AgentTasksAction::RunNow {
                task_id: task_id.clone(),
            },
            &tasks,
            &rev,
        );
        assert_eq!(result["ok"], true);
        let guard = tasks.lock().unwrap();
        assert_eq!(guard[0].status, "completed");
        assert!(guard[0].last_run_at.is_some());
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
        );
        assert_eq!(result["ok"], false);
    }
}
