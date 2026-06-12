//! Tasks substate — Step 6 of the god-root consolidation.
//!
//! Owns the single slot that was previously mirrored between
//! `PodcastHandle` and `PodcastHostOpHandler`:
//!
//! * `tasks` — the `Vec<AgentTaskSummary>` cache slot.  Durability:
//!   **Session** (the slot is an in-memory projection; write-through
//!   persistence is handled by `crate::store::agent_tasks::save_agent_tasks`
//!   inside the `handle` method — the persisted file is the canonical source).
//!
//! The free functions `handle_tasks_action` / `handle_tasks_action_with_persist`
//! in `crate::tasks_handler` are re-exposed as `TasksState::handle` so the
//! router arm calls `self.state.tasks.handle(action, app)` instead of
//! constructing the Arcs and closures inline inside `task_actions.rs`.
//!
//! ## Persistence model
//!
//! At registration time (`register.rs`) the tasks slot is seeded with either:
//! * the persisted list from `store::agent_tasks::load_agent_tasks` (cold launch
//!   with an existing data dir), or
//! * `tasks_handler::default_seed()` (first launch with no sidecar file).
//!
//! This seeding moves from `register.rs` into `TasksState::new` (receives the
//! data dir path as an optional parameter).
//!
//! After every mutating action the slot is persisted back via `save_agent_tasks`
//! — same write-through as before, but the `store.data_dir()` lookup and the
//! closure construction live here rather than in `task_actions.rs`.
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.

use std::ffi::CString;
use std::path::Path;
use std::sync::{Arc, Mutex};

use nmp_ffi::NmpApp;

use crate::ffi::actions::tasks_module::AgentTasksAction;
use crate::ffi::projections::AgentTaskSummary;
use crate::state::slot::Session;
use crate::state::{Infra, Slot};
use crate::store::PodcastStore;
use crate::tasks_handler;

/// Tasks feature substate.
///
/// Constructed once in `PodcastAppState::new` and referenced via
/// `state.tasks` on both seams.
pub struct TasksState {
    /// In-memory agent-task list.  Write-through to the JSON sidecar; the
    /// slot itself is Session durability (rebuilt from the file on cold launch).
    pub tasks: Slot<Vec<AgentTaskSummary>, Session>,
    /// Rev + signal + runtime.
    infra: Infra,
    /// The canonical persisted store — used to look up `data_dir()` for
    /// the write-through persistence path.
    store: Arc<Mutex<PodcastStore>>,
}

impl TasksState {
    /// Production constructor — called from `PodcastAppState::new`.
    ///
    /// Seed precedence:
    ///  1. Persisted sidecar (`load_agent_tasks`) — present after first run.
    ///  2. `default_seed()` — first-launch fallback so the iOS UI has rows.
    pub fn new(infra: Infra, store: Arc<Mutex<PodcastStore>>) -> Self {
        // Try to load from the persisted sidecar if a data dir is configured.
        let seed = store
            .lock()
            .ok()
            .and_then(|s| s.data_dir().map(Path::to_path_buf))
            .and_then(|dir| crate::store::agent_tasks::load_agent_tasks(&dir))
            .unwrap_or_else(tasks_handler::default_seed);

        Self {
            tasks: Slot::new(seed),
            infra,
            store,
        }
    }

    /// Test constructor — no `NmpApp` needed; seeds with defaults.
    #[cfg(test)]
    pub fn for_test() -> Self {
        use crate::state::Infra;
        Self {
            tasks: Slot::new(tasks_handler::default_seed()),
            infra: Infra::for_test(),
            store: Arc::new(Mutex::new(PodcastStore::new())),
        }
    }

    // ── Snapshot projection ───────────────────────────────────────────────

    /// Clone the current task list for the snapshot projection.
    ///
    /// `build_podcast_update` calls this instead of locking
    /// `handle.agent_tasks` directly.
    pub fn tasks_snapshot(&self) -> Vec<AgentTaskSummary> {
        self.tasks.lock().ok().map(|t| t.clone()).unwrap_or_default()
    }

    // ── Action handler ────────────────────────────────────────────────────

    /// Route a single `podcast.tasks.*` action.
    ///
    /// Replaces `PodcastHostOpHandler::handle_task_action`.  `app` is needed
    /// only for the `RunNow` dispatch path (wraps `nmp_app_dispatch_action`);
    /// all other ops ignore it.
    pub fn handle(&self, action: AgentTasksAction, app: *mut NmpApp) -> serde_json::Value {
        let tasks = self.tasks.share();
        let rev = self.infra.rev.clone();

        // Build the dispatch closure (mirrors task_actions.rs inline closure).
        // If `app` is null (tests without a live kernel) dispatch always returns
        // `false` and the task stays in `"running"`.
        let dispatch = move |namespace: &str, body: &str| -> bool {
            if app.is_null() {
                return false;
            }
            let (Ok(ns_c), Ok(body_c)) = (CString::new(namespace), CString::new(body)) else {
                return false;
            };
            let raw = nmp_ffi::nmp_app_dispatch_action(app, ns_c.as_ptr(), body_c.as_ptr());
            if raw.is_null() {
                return false;
            }
            // SAFETY: `raw` is a heap-owned NUL-terminated C string minted by
            // `nmp_app_dispatch_action`; read it, then return ownership.
            let envelope = unsafe { std::ffi::CStr::from_ptr(raw) }
                .to_string_lossy()
                .into_owned();
            nmp_ffi::nmp_free_string(raw);
            envelope.contains("\"correlation_id\"")
        };

        // Persist closure: write-through to the JSON sidecar when the task
        // list changes.  Mirrors the closure in `task_actions.rs`.
        let data_dir = self
            .store
            .lock()
            .ok()
            .and_then(|s| s.data_dir().map(Path::to_path_buf));
        let persist = |snapshot: &[AgentTaskSummary]| {
            let Some(dir) = data_dir.as_deref() else {
                return;
            };
            let _ = crate::store::agent_tasks::save_agent_tasks(dir, snapshot);
        };

        tasks_handler::handle_tasks_action_with_persist(
            action,
            &tasks,
            &rev,
            Some(&dispatch),
            Some(&persist),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::ffi::actions::tasks_module::AgentTasksAction;

    use super::*;

    #[test]
    fn tasks_snapshot_returns_default_seed() {
        let state = TasksState::for_test();
        let snap = state.tasks_snapshot();
        assert!(!snap.is_empty(), "default seed should be non-empty");
        assert_eq!(snap[0].schedule, "daily");
    }

    #[test]
    fn create_task_bumps_rev() {
        let state = TasksState::for_test();
        let rev0 = state.infra.rev();

        let out = state.handle(
            AgentTasksAction::Create {
                title: "My Task".into(),
                description: None,
                action_namespace: "podcast.inbox".into(),
                action_body: r#"{"op":"triage"}"#.into(),
                schedule: "daily".into(),
            },
            std::ptr::null_mut(),
        );
        assert_eq!(out["ok"], true);
        assert!(out["task_id"].is_string());
        assert!(state.infra.rev() > rev0, "create must bump rev");
        // Seed + newly created
        assert_eq!(state.tasks_snapshot().len(), 2);
    }

    #[test]
    fn delete_task_bumps_rev() {
        let state = TasksState::for_test();
        let snap = state.tasks_snapshot();
        let task_id = snap[0].id.clone();
        let rev0 = state.infra.rev();

        let out = state.handle(
            AgentTasksAction::Delete {
                task_id: task_id.clone(),
            },
            std::ptr::null_mut(),
        );
        assert_eq!(out["ok"], true);
        assert!(state.infra.rev() > rev0, "delete must bump rev");
        assert!(state.tasks_snapshot().iter().all(|t| t.id != task_id));
    }

    #[test]
    fn enable_disable_task() {
        let state = TasksState::for_test();
        let task_id = state.tasks_snapshot()[0].id.clone();

        let out_disable = state.handle(
            AgentTasksAction::Disable {
                task_id: task_id.clone(),
            },
            std::ptr::null_mut(),
        );
        assert_eq!(out_disable["ok"], true);
        assert!(!state.tasks_snapshot()[0].is_enabled);

        let out_enable = state.handle(
            AgentTasksAction::Enable {
                task_id: task_id.clone(),
            },
            std::ptr::null_mut(),
        );
        assert_eq!(out_enable["ok"], true);
        assert!(state.tasks_snapshot()[0].is_enabled);
    }
}
