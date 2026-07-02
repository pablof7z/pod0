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
//! * `tasks_handler::default_seed(now)` (first launch with no sidecar file).
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

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nmp_native_runtime::NmpApp;
use tokio::task::JoinHandle;

use crate::ffi::actions::tasks_module::AgentTasksAction;
use crate::ffi::projections::AgentTaskSummary;
use crate::state::slot::Session;
use crate::state::{Infra, Slot};
use crate::store::PodcastStore;
use crate::tasks_handler;

/// Interval between kernel-owned task-due checks (D9 / D13).
///
/// 60 seconds is short enough for minute-granularity task scheduling while
/// cheap enough to leave the Tokio runtime idle (the check is O(n) on a
/// small list).  Slice 2 will delete the host-side foreground poll and may
/// adjust this interval.
const TICK_INTERVAL: Duration = Duration::from_secs(60);

/// Tasks feature substate.
///
/// Constructed once in `PodcastAppState::new` and referenced via
/// `state.tasks` on both seams.
///
/// ## Shutdown fence — CRITICAL (UAF prevention)
///
/// [`Self::start_ticker`] spawns a Tokio task that dereferences `*mut NmpApp`
/// (the `nmp_app_dispatch_action` call).  Mirroring [`crate::state::voice`],
/// the spawned task's [`JoinHandle`] is retained in `ticker` and
/// [`Self::shutdown`] aborts + joins it.  `nmp_app_podcast_unregister` MUST call
/// `shutdown()` BEFORE `nmp_app_free`, so no spawned task can dereference `app`
/// after the allocation is freed.  The task captures only a
/// [`crate::state::BumpHandle`] (NOT a full `Infra`), so it holds **no** strong
/// ref to the runtime it runs on — without that, the runtime would never drop
/// and the fence could never run.
pub struct TasksState {
    /// In-memory agent-task list.  Write-through to the JSON sidecar; the
    /// slot itself is Session durability (rebuilt from the file on cold launch).
    pub tasks: Slot<Vec<AgentTaskSummary>, Session>,
    /// Rev + signal + runtime.
    infra: Infra,
    /// The canonical persisted store — used to look up `data_dir()` for
    /// the write-through persistence path.
    store: Arc<Mutex<PodcastStore>>,
    /// Join handle for the spawned periodic ticker.  `None` until
    /// [`Self::start_ticker`] runs; taken + joined by [`Self::shutdown`].
    /// `Mutex` gives the `&self` methods interior mutability (the substate is
    /// shared behind `Arc<PodcastAppState>`).
    ticker: Mutex<Option<JoinHandle<()>>>,
    /// Set by [`Self::shutdown`].  Checked at the top of each tick so a task
    /// that wakes during teardown returns without dereferencing `app` — a
    /// belt-and-braces guard alongside the abort + join fence.
    shutting_down: Arc<AtomicBool>,
}

impl TasksState {
    /// Production constructor — called from `PodcastAppState::new`.
    ///
    /// Seed precedence:
    ///  1. Persisted sidecar (`load_agent_tasks`) — present after first run.
    ///  2. `default_seed(now)` — first-launch fallback so the iOS UI has rows.
    pub fn new(infra: Infra, store: Arc<Mutex<PodcastStore>>) -> Self {
        // Try to load from the persisted sidecar if a data dir is configured.
        let seed = store
            .lock()
            .ok()
            .and_then(|s| s.data_dir().map(Path::to_path_buf))
            .and_then(|dir| crate::store::agent_tasks::load_agent_tasks(&dir))
            .unwrap_or_else(|| {
                // D9: compute wall-clock once at the entry point; pass into helper.
                tasks_handler::default_seed(chrono::Utc::now().timestamp())
            });

        Self {
            tasks: Slot::new(seed),
            infra,
            store,
            ticker: Mutex::new(None),
            shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Test constructor — no `NmpApp` needed; seeds with defaults.
    #[cfg(test)]
    pub fn for_test() -> Self {
        use crate::state::Infra;
        Self {
            tasks: Slot::new(tasks_handler::default_seed(chrono::Utc::now().timestamp())),
            infra: Infra::for_test(),
            store: Arc::new(Mutex::new(PodcastStore::new())),
            ticker: Mutex::new(None),
            shutting_down: Arc::new(AtomicBool::new(false)),
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

    // ── Kernel-owned periodic tick ────────────────────────────────────────

    /// Spawn the kernel-owned periodic task-firing loop.
    ///
    /// Called from `register.rs` after the `PodcastHandle` is sealed so the
    /// real `*mut NmpApp` pointer is available.  The loop wakes every
    /// [`TICK_INTERVAL`], reads kernel wall-clock (`Utc::now()`), and fires any
    /// tasks whose `next_run_at <= now` via the same
    /// [`crate::tasks_handler::maybe_run_due_tasks`] → `run_task_by_id` path the
    /// `RunDue` / `RunNow` actions use (D13: single firing path).
    ///
    /// ## Shutdown fence (UAF prevention) — see the type-level docs
    ///
    /// The spawned task dereferences `app` (`nmp_app_dispatch_action`) on a
    /// Tokio worker thread.  Its [`JoinHandle`] is retained in `self.ticker` and
    /// [`Self::shutdown`] aborts + joins it; `nmp_app_podcast_unregister`
    /// calls `shutdown()` BEFORE `nmp_app_free`.  The task captures only a
    /// [`crate::state::BumpHandle`] — NOT a full `Infra` — so it holds **no**
    /// strong ref to the runtime it runs on (a full `Infra` capture would pin
    /// `Arc<Runtime>` alive, so the runtime would never drop, the task would
    /// never stop, and it would dereference a freed `NmpApp`).
    ///
    /// ## Idempotency with host `RunDue`
    ///
    /// The host's foreground `run_due` calls the same `run_task_by_id` under the
    /// same `tasks` lock.  `run_task_by_id` advances `next_run_at` past `now`
    /// AND leaves `status == "running"` under that lock before dispatching, and
    /// both the kernel tick and the `RunDue` filter skip `status == "running"`
    /// + already-advanced tasks — so whichever caller wins the lock first, the
    /// other finds no due task.  No double-fire is possible.  Slice 2 deletes
    /// the host poll; this tick path is the sole survivor.
    pub fn start_ticker(&self, app: *mut NmpApp) {
        // Capture the pointer as a `usize` so the dispatch closure is `Send`.
        // The pointer is materialised only INSIDE the closure body (a
        // synchronous call, never held across an `.await`), so the spawned
        // future stays `Send`.
        let app_addr = app as usize;
        let dispatch = move |namespace: &str, body: &str| -> bool {
            // app_ptr is fenced by `shutdown` (abort + join before
            // `nmp_app_free`); dispatch only enqueues (D8: non-blocking).
            let app_ptr = app_addr as *mut NmpApp;
            crate::dispatch_bytes::dispatch_action_bytes_for(app_ptr, namespace, body).is_ok()
        };

        self.spawn_ticker_loop(TICK_INTERVAL, dispatch);
    }

    /// Generic ticker spawn used by both production ([`Self::start_ticker`],
    /// which injects the FFI dispatch) and tests (which inject a counting
    /// closure + a short interval).
    ///
    /// Captures ONLY capture-safe primitives — `tasks`/`rev`/`BumpHandle`/
    /// `shutting_down` — and the caller-supplied `dispatch`.  It deliberately
    /// does NOT capture `Infra` (and thus `Arc<Runtime>`): the spawned task runs
    /// ON `self.infra.runtime`, so holding a strong runtime ref inside it would
    /// stop the runtime from ever dropping (UAF at teardown).
    ///
    /// Single-spawn guarded: a second call is a no-op while a ticker is live.
    fn spawn_ticker_loop<D>(&self, interval: Duration, dispatch: D)
    where
        D: Fn(&str, &str) -> bool + Send + 'static,
    {
        // Single-spawn guard: never run two tickers.
        {
            let guard = match self.ticker.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            if guard.is_some() {
                return;
            }
        }

        let tasks = self.tasks.share();
        let rev = self.infra.rev.clone();
        // Capture-safe: BumpHandle holds NO Arc<Runtime> (see Infra::bump_handle).
        let bump = self.infra.bump_handle();
        let shutting_down = Arc::clone(&self.shutting_down);

        let handle = self.infra.runtime.spawn(async move {
            loop {
                tokio::time::sleep(interval).await;

                // Belt-and-braces: a task that wakes during teardown returns
                // before dereferencing `app`.  The real fence is shutdown's
                // abort + join; this just narrows the window.
                if shutting_down.load(Ordering::SeqCst) {
                    return;
                }

                let now_unix = chrono::Utc::now().timestamp();
                let fired = tasks_handler::maybe_run_due_tasks(
                    &tasks,
                    &rev,
                    Some(&dispatch),
                    now_unix,
                );
                if fired > 0 {
                    // Bump Domain::Tasks rev (+ global) so the snapshot
                    // projection delivers the updated task statuses.
                    bump.bump();
                }
            }
        });

        if let Ok(mut guard) = self.ticker.lock() {
            *guard = Some(handle);
        }
    }

    /// Fence the periodic ticker before the owning `NmpApp` frees.
    ///
    /// Sets `shutting_down` (so a task waking mid-teardown no-ops), then aborts
    /// + joins the ticker handle.  Aborting cancels the task at its `.await`
    /// (the `sleep`) promptly — even though the production interval is 60 s, so
    /// teardown never blocks waiting for the next tick.  A task already past the
    /// sleep runs its bounded, await-free `maybe_run_due_tasks` to completion
    /// and the join waits the few microseconds for it.  Either way, when
    /// `shutdown` returns no spawned task will dereference `app` — so it is
    /// sound for `nmp_app_podcast_unregister` to call this immediately before
    /// `nmp_app_free`.
    ///
    /// The join runs via [`tokio::runtime::Runtime::block_on`], so `shutdown`
    /// MUST be called from a thread that is NOT inside this runtime (the
    /// FFI/Swift thread running `unregister` qualifies) — identical to
    /// [`crate::state::voice::VoiceSubstate::shutdown`].
    pub fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        let handle = match self.ticker.lock() {
            Ok(mut g) => g.take(),
            Err(_) => return,
        };
        if let Some(handle) = handle {
            handle.abort();
            // Aborting cancels the task at its `.await`, so this join resolves
            // promptly (no 60 s wait).
            self.infra.runtime.block_on(async {
                let _ = handle.await;
            });
        }
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
            crate::dispatch_bytes::dispatch_action_bytes_for(app, namespace, body).is_ok()
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

// SAFETY: `TasksState` stores no raw `*mut NmpApp` in any field — the
// `start_ticker` method captures the pointer as a `usize` inside the spawned
// task (materialised only for the synchronous `nmp_app_dispatch_action` call,
// never across an `.await`).  All struct fields are already `Send + Sync`
// (`Slot`/`Arc`/`Mutex`/`Infra`).  The off-thread `app` dereference is fenced
// by [`Self::shutdown`] (abort + join), called from `nmp_app_podcast_unregister`
// BEFORE `nmp_app_free` — the same fence the voice manager uses.  The spawned
// task captures only a `BumpHandle` (no `Arc<Runtime>`), so the runtime can
// actually drop and the fence can run.
unsafe impl Send for TasksState {}
unsafe impl Sync for TasksState {}

#[cfg(test)]
#[path = "tasks_tests.rs"]
mod tests;
