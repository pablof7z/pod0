//! Headless kernel integration test.
//!
//! Boots the real Rust kernel through [`podcast_tui::runtime::AppRuntime`]
//! (the same HTTP + audio capability harness the TUI uses) with **no
//! terminal UI**, then exercises the full subscribe → snapshot → mutate →
//! re-read round-trip against a live RSS feed.
//!
//! This is an *integration* test, not a unit test: the subscribe step makes
//! a real network request. It is intentionally excluded from `cargo test`
//! (it lives as a `[[bin]]`) and is driven by
//! `tests/integration/run_tui_integration.sh`.
//!
//! ## Contract notes
//!
//! Every mutation (`subscribe`, queue add/remove, mark played/unplayed,
//! settings) is processed asynchronously on the kernel actor thread, which
//! fires an [`NmpEvent`] on the channel once the new snapshot is ready.
//! Reading `podcast_update()` on the line *after* a dispatch races the actor
//! and usually observes pre-mutation state. So every assertion routes through
//! [`wait_until`], which blocks on the channel (with a deadline) and re-reads
//! the *real* snapshot in its predicate — making it immune to stale or
//! spurious channel events without any draining.
//!
//! Failures `eprintln!` and `process::exit(1)` rather than `panic!`/`assert!`
//! (which would exit with code 101) so the runner sees the documented 1.

use std::process;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nmp_app_podcast::ffi::PodcastUpdate;
use podcast_tui::bridge::NmpEvent;
use podcast_tui::runtime::AppRuntime;
use serde_json::json;

/// Stable, auth-free, reliably-non-empty RSS feed (TWiT Network).
const FEED_URL: &str = "https://feeds.twit.tv/twit.xml";

/// Per-assertion convergence budget.
const CONVERGENCE_TIMEOUT: Duration = Duration::from_secs(30);

fn main() {
    match run() {
        Ok(()) => {
            println!("ALL ASSERTIONS PASSED");
            process::exit(0);
        }
        Err(msg) => {
            eprintln!("INTEGRATION TEST FAILED: {msg}");
            process::exit(1);
        }
    }
}

fn run() -> Result<(), String> {
    let data_dir = make_temp_dir()?;
    println!("[integration] data dir: {data_dir}");

    // Run the body in a closure so we always clean up the temp dir, even on
    // an early error return.
    let result = run_body(&data_dir);

    let _ = std::fs::remove_dir_all(&data_dir);
    result
}

fn run_body(data_dir: &str) -> Result<(), String> {
    println!("[integration] booting kernel runtime…");
    let (runtime, rx) =
        AppRuntime::new(&Some(data_dir.to_string())).map_err(|e| format!("boot failed: {e}"))?;

    // ---- 2. Subscribe to a well-known feed (real network) -----------------
    println!("[integration] subscribing to {FEED_URL}");
    runtime
        .subscribe(FEED_URL)
        .map_err(|e| format!("subscribe dispatch failed: {e}"))?;

    // ---- 3 + 4. Wait for the library to populate, then assert ------------
    println!("[integration] waiting for library to populate (≤30s)…");
    let snapshot = wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        first_library_episode_id(u).is_some()
    })
    .ok_or_else(|| {
        "timed out waiting for the subscribed feed to surface ≥1 episode in `library`".to_string()
    })?;

    let podcast = snapshot
        .library
        .first()
        .ok_or_else(|| "library is empty after subscribe".to_string())?;
    if podcast.episode_count == 0 {
        return Err(format!(
            "podcast '{}' has episode_count == 0",
            podcast.title
        ));
    }
    let episode_id = first_library_episode_id(&snapshot)
        .ok_or_else(|| "no episodes embedded in the library podcast".to_string())?;
    println!(
        "[integration] PASS: podcast '{}' in library, {} episodes (target ep {})",
        podcast.title, podcast.episode_count, episode_id
    );

    // ---- 5 + 6. Add the first episode to the queue, assert present -------
    println!("[integration] adding episode to queue…");
    runtime
        .add_to_queue(&episode_id)
        .map_err(|e| format!("add_to_queue dispatch failed: {e}"))?;
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        queue_contains(u, &episode_id)
    })
    .ok_or_else(|| format!("timed out waiting for episode {episode_id} to enter the queue"))?;
    println!("[integration] PASS: queue contains the episode");

    // ---- 7 + 8. Remove it, assert the queue is empty ---------------------
    println!("[integration] removing episode from queue…");
    runtime
        .remove_from_queue(&episode_id)
        .map_err(|e| format!("remove_from_queue dispatch failed: {e}"))?;
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| u.queue.is_empty())
        .ok_or_else(|| "timed out waiting for the queue to empty".to_string())?;
    println!("[integration] PASS: queue is empty");

    // ---- 9 + 10. Mark played, assert played == true ----------------------
    println!("[integration] marking episode played…");
    runtime
        .mark_played(&episode_id)
        .map_err(|e| format!("mark_played dispatch failed: {e}"))?;
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        episode_played(u, &episode_id) == Some(true)
    })
    .ok_or_else(|| format!("timed out waiting for episode {episode_id} to read played == true"))?;
    println!("[integration] PASS: episode.played == true");

    // ---- 11 + 12. Mark unplayed, assert played == false ------------------
    println!("[integration] marking episode unplayed…");
    runtime
        .mark_unplayed(&episode_id)
        .map_err(|e| format!("mark_unplayed dispatch failed: {e}"))?;
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        episode_played(u, &episode_id) == Some(false)
    })
    .ok_or_else(|| format!("timed out waiting for episode {episode_id} to read played == false"))?;
    println!("[integration] PASS: episode.played == false");

    // ---- 13. Download control routing smoke ------------------------------
    println!("[integration] dispatching download control no-ops...");
    runtime
        .pause_download(&episode_id)
        .map_err(|e| format!("pause_download dispatch failed: {e}"))?;
    runtime
        .resume_download(&episode_id)
        .map_err(|e| format!("resume_download dispatch failed: {e}"))?;
    runtime
        .cancel_download(&episode_id)
        .map_err(|e| format!("cancel_download dispatch failed: {e}"))?;
    runtime
        .cancel_all_downloads()
        .map_err(|e| format!("cancel_all_downloads dispatch failed: {e}"))?;
    runtime
        .delete_download(&episode_id)
        .map_err(|e| format!("delete_download dispatch failed: {e}"))?;
    println!("[integration] PASS: download controls dispatched");

    // ---- 14. Episode detail control routing smoke ------------------------
    println!("[integration] dispatching episode detail controls...");
    runtime
        .fetch_transcript(&episode_id)
        .map_err(|e| format!("fetch_transcript dispatch failed: {e}"))?;
    runtime
        .fetch_chapters(&episode_id)
        .map_err(|e| format!("fetch_chapters dispatch failed: {e}"))?;
    runtime
        .compile_chapters(&episode_id)
        .map_err(|e| format!("compile_chapters dispatch failed: {e}"))?;
    runtime
        .fetch_comments(&episode_id)
        .map_err(|e| format!("fetch_comments dispatch failed: {e}"))?;
    runtime
        .summarize_episode(&episode_id)
        .map_err(|e| format!("summarize_episode dispatch failed: {e}"))?;
    runtime
        .reset_progress(&episode_id)
        .map_err(|e| format!("reset_progress dispatch failed: {e}"))?;
    runtime
        .set_sleep_timer(Some(15 * 60))
        .map_err(|e| format!("set_sleep_timer dispatch failed: {e}"))?;
    runtime
        .set_sleep_timer(None)
        .map_err(|e| format!("cancel_sleep_timer dispatch failed: {e}"))?;
    println!("[integration] PASS: episode detail controls dispatched");

    // ---- 13 + 14. set_default_playback_rate 1.5, assert speed 1.5 --------
    println!("[integration] dispatching settings set_default_playback_rate 1.5…");
    let action = json!({ "op": "set_default_playback_rate", "rate": 1.5 });
    runtime
        .dispatch_action_value("podcast.settings", &action)
        .map_err(|e| format!("set_default_playback_rate dispatch failed: {e}"))?;
    // 1.5 is exactly representable as f64, so an exact compare is safe here.
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        u.settings.default_playback_rate == 1.5
    })
    .ok_or_else(|| "timed out waiting for settings.default_playback_rate == 1.5".to_string())?;
    println!("[integration] PASS: settings.default_playback_rate == 1.5");

    // ---- 15. Relay editor settings round-trip ----------------------------
    let relay_url = "wss://tui-integration.invalid";
    println!("[integration] adding configured relay...");
    runtime
        .add_relay(relay_url, "read")
        .map_err(|e| format!("add_relay dispatch failed: {e}"))?;
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        relay_role(u, relay_url).as_deref() == Some("read")
    })
    .ok_or_else(|| "timed out waiting for relay add".to_string())?;
    println!("[integration] PASS: relay added");

    runtime
        .set_relay_role(relay_url, "write")
        .map_err(|e| format!("set_relay_role dispatch failed: {e}"))?;
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        relay_role(u, relay_url).as_deref() == Some("write")
    })
    .ok_or_else(|| "timed out waiting for relay role update".to_string())?;
    println!("[integration] PASS: relay role updated");

    runtime
        .remove_relay(relay_url)
        .map_err(|e| format!("remove_relay dispatch failed: {e}"))?;
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        relay_role(u, relay_url).is_none()
    })
    .ok_or_else(|| "timed out waiting for relay removal".to_string())?;
    println!("[integration] PASS: relay removed");

    // ---- 15 + 16. Agent chat round-trip ---------------------------------
    println!("[integration] sending agent chat message…");
    runtime
        .send_agent_message("summarize my queue")
        .map_err(|e| format!("agent send dispatch failed: {e}"))?;
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        u.agent
            .as_ref()
            .map(|agent| agent.messages.len() >= 2)
            .unwrap_or(false)
    })
    .ok_or_else(|| "timed out waiting for agent messages".to_string())?;
    println!("[integration] PASS: agent chat projected messages");

    // ---- 17 + 18. Agent memory CRUD -------------------------------------
    println!("[integration] remembering agent memory fact…");
    runtime
        .remember_memory("tui_test_pref", "agent parity")
        .map_err(|e| format!("remember memory dispatch failed: {e}"))?;
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        memory_contains(u, "tui_test_pref", "agent parity")
    })
    .ok_or_else(|| "timed out waiting for memory fact".to_string())?;
    println!("[integration] PASS: memory fact projected");

    println!("[integration] forgetting agent memory fact…");
    runtime
        .forget_memory("tui_test_pref")
        .map_err(|e| format!("forget memory dispatch failed: {e}"))?;
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        !u.memory_facts
            .iter()
            .any(|fact| fact.key == "tui_test_pref")
    })
    .ok_or_else(|| "timed out waiting for memory fact removal".to_string())?;
    println!("[integration] PASS: memory fact removed");

    // ---- 19 + 20. Agent task CRUD/run -----------------------------------
    println!("[integration] creating agent task…");
    runtime
        .create_agent_task(
            "TUI Smoke Task",
            "once",
            "clear_agent",
            Some("integration smoke"),
        )
        .map_err(|e| format!("create task dispatch failed: {e}"))?;
    let snapshot = wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        agent_task_id(u, "TUI Smoke Task").is_some()
    })
    .ok_or_else(|| "timed out waiting for created task".to_string())?;
    let task_id = agent_task_id(&snapshot, "TUI Smoke Task")
        .ok_or_else(|| "created task missing from satisfying snapshot".to_string())?;
    println!("[integration] PASS: agent task projected ({task_id})");

    runtime
        .disable_agent_task(&task_id)
        .map_err(|e| format!("disable task dispatch failed: {e}"))?;
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        agent_task_enabled(u, &task_id) == Some(false)
    })
    .ok_or_else(|| "timed out waiting for disabled task".to_string())?;
    println!("[integration] PASS: agent task disabled");

    runtime
        .enable_agent_task(&task_id)
        .map_err(|e| format!("enable task dispatch failed: {e}"))?;
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        agent_task_enabled(u, &task_id) == Some(true)
    })
    .ok_or_else(|| "timed out waiting for enabled task".to_string())?;
    println!("[integration] PASS: agent task enabled");

    runtime
        .run_agent_task_now(&task_id)
        .map_err(|e| format!("run task dispatch failed: {e}"))?;
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        agent_task_status(u, &task_id).as_deref() == Some("completed")
    })
    .ok_or_else(|| "timed out waiting for task run completion".to_string())?;
    println!("[integration] PASS: agent task run_now completed");

    runtime
        .delete_agent_task(&task_id)
        .map_err(|e| format!("delete task dispatch failed: {e}"))?;
    wait_until(&runtime, &rx, CONVERGENCE_TIMEOUT, |u| {
        agent_task_status(u, &task_id).is_none()
    })
    .ok_or_else(|| "timed out waiting for task deletion".to_string())?;
    println!("[integration] PASS: agent task deleted");

    // `runtime` drops here, unregistering the kernel cleanly before the
    // temp dir is removed by the caller.
    Ok(())
}

/// Block until `predicate(snapshot)` holds or the deadline elapses.
///
/// The predicate re-reads the *current* snapshot from the kernel handle, so a
/// stale or spurious channel wakeup simply re-evaluates against real state —
/// no event draining is required. Returns the satisfying snapshot, or `None`
/// on timeout (including a closed channel, which means the kernel went away).
fn wait_until(
    runtime: &AppRuntime,
    rx: &Receiver<NmpEvent>,
    timeout: Duration,
    predicate: impl Fn(&PodcastUpdate) -> bool,
) -> Option<PodcastUpdate> {
    // Fast path: the actor may already have produced a satisfying snapshot.
    if let Some(update) = runtime.podcast_update() {
        if predicate(&update) {
            return Some(update);
        }
    }

    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match rx.recv_timeout(remaining) {
            Ok(_event) => {
                if let Some(update) = runtime.podcast_update() {
                    if predicate(&update) {
                        return Some(update);
                    }
                }
            }
            // RecvTimeoutError::Timeout (deadline) or Disconnected (kernel
            // gone): either way we cannot make further progress.
            Err(_) => return None,
        }
    }
}

/// Id of the first episode of the first library podcast, if any.
fn first_library_episode_id(update: &PodcastUpdate) -> Option<String> {
    update
        .library
        .first()
        .and_then(|p| p.episodes.first())
        .map(|e| e.id.clone())
}

/// Whether the playback queue contains an episode with `episode_id`.
fn queue_contains(update: &PodcastUpdate, episode_id: &str) -> bool {
    update.queue.iter().any(|e| e.id == episode_id)
}

/// The `played` flag for `episode_id` as seen anywhere in the library, or
/// `None` when the episode is not present in the snapshot yet.
fn episode_played(update: &PodcastUpdate, episode_id: &str) -> Option<bool> {
    update
        .library
        .iter()
        .flat_map(|p| p.episodes.iter())
        .find(|e| e.id == episode_id)
        .map(|e| e.played)
}

fn memory_contains(update: &PodcastUpdate, key: &str, value: &str) -> bool {
    update
        .memory_facts
        .iter()
        .any(|fact| fact.key == key && fact.value == value)
}

fn agent_task_id(update: &PodcastUpdate, title: &str) -> Option<String> {
    update
        .agent_tasks
        .iter()
        .find(|task| task.title == title)
        .map(|task| task.id.clone())
}

fn agent_task_enabled(update: &PodcastUpdate, task_id: &str) -> Option<bool> {
    update
        .agent_tasks
        .iter()
        .find(|task| task.id == task_id)
        .map(|task| task.is_enabled)
}

fn agent_task_status(update: &PodcastUpdate, task_id: &str) -> Option<String> {
    update
        .agent_tasks
        .iter()
        .find(|task| task.id == task_id)
        .map(|task| task.status.clone())
}

fn relay_role(update: &PodcastUpdate, url: &str) -> Option<String> {
    update
        .configured_relays
        .iter()
        .find(|relay| relay.url == url)
        .map(|relay| relay.role.clone())
}

/// Create a unique, hermetic temp data dir under the system temp directory.
fn make_temp_dir() -> Result<String, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock error: {e}"))?
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "podcast-tui-integration-{}-{}",
        process::id(),
        nanos
    ));
    std::fs::create_dir_all(&dir).map_err(|e| format!("create temp dir failed: {e}"))?;
    dir.to_str()
        .map(str::to_string)
        .ok_or_else(|| "temp dir path is not valid UTF-8".to_string())
}
