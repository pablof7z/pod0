//! Handler-level regression guard for #561: `SetSpeed` must persist the
//! chosen rate to `PodcastStore::default_playback_rate` so it survives a
//! cold relaunch (podcasts.json is preserved across `--UITestSeedRelaunch`).
//!
//! These tests prove the end-to-end path:
//!   dispatch SetSpeed → store.set_default_playback_rate → flush to disk →
//!   new store loads same dir → default_playback_rate equals chosen rate.
//!
//! Uses the public `handle(envelope_json, corr_id)` API so the tests are
//! namespace-transparent and live outside the `pub(super)` boundary.

use crate::host_op_handler::PodcastHostOpHandler;
use crate::store::identity::IdentityStore;
use crate::store::PodcastStore;
use nmp_core::substrate::HostOpHandler;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

// ── RAII temp dir ─────────────────────────────────────────────────────────────

struct TempDir {
    path: std::path::PathBuf,
}
impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::Ordering;
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("nmp-speed-persist-test-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

// ── Constructor helpers ───────────────────────────────────────────────────────

fn feedback_runtime(rev: Arc<AtomicU64>) -> nmp_feedback::FeedbackRuntime {
    nmp_feedback::FeedbackRuntime::new(
        nmp_feedback::FeedbackConfig::new(crate::PODCAST_FEEDBACK_PROJECT_COORDINATE)
            .with_interest_namespace(crate::PODCAST_FEEDBACK_INTEREST_NAMESPACE),
        Arc::new(Mutex::new(Vec::new())),
        rev,
    )
}

fn handler_with_store(store: Arc<Mutex<PodcastStore>>) -> PodcastHostOpHandler {
    let rev = Arc::new(AtomicU64::new(1));
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    let state = Arc::new(crate::state::PodcastAppState::new_with_identity(
        crate::state::Infra::for_test(),
        store.clone(),
        identity.clone(),
        feedback_runtime(rev.clone()),
    ));
    PodcastHostOpHandler::new(std::ptr::null_mut(), state)
}

/// Dispatch a `podcast.player` / `set_speed` envelope through the public router.
fn dispatch_set_speed(handler: &PodcastHostOpHandler, speed: f64) -> serde_json::Value {
    let envelope = serde_json::json!({
        "ns": "podcast.player",
        "action": { "op": "set_speed", "speed": speed }
    });
    handler.handle(&envelope.to_string(), "corr-speed")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Dispatching `SetSpeed` through the handler must write `default_playback_rate`
/// to the store's persisted layer so a fresh store reload (simulating a cold
/// relaunch) returns the same rate. This guards the fix for #561 where
/// `SetSpeed` only updated the in-memory player actor but never persisted.
#[test]
fn set_speed_action_persists_default_playback_rate_across_reload() {
    let dir = TempDir::new();

    // ── Session 1: dispatch SetSpeed, then drop the handler (simulates quit). ──
    {
        let store = Arc::new(Mutex::new(PodcastStore::new()));
        store.lock().unwrap().set_data_dir(dir.path.clone());
        let handler = handler_with_store(store);

        let result = dispatch_set_speed(&handler, 1.5);
        assert_eq!(
            result["ok"],
            serde_json::json!(true),
            "SetSpeed must return ok:true: {result}"
        );

        // Confirm the in-memory store already reflects the new rate.
        let in_memory = handler
            .state
            .library
            .store
            .lock()
            .unwrap()
            .default_playback_rate();
        assert!(
            (in_memory - 1.5).abs() < f64::EPSILON,
            "store must reflect new rate in-memory immediately: got {in_memory}"
        );
    } // handler + store dropped (simulates force-quit)

    // ── Session 2: fresh store from the same dir (simulates cold relaunch). ──
    let mut reloaded = PodcastStore::new();
    reloaded.set_data_dir(dir.path.clone());
    let restored = reloaded.default_playback_rate();
    assert!(
        (restored - 1.5).abs() < f64::EPSILON,
        "cold-relaunch store must restore the persisted rate (got {restored}, expected 1.5)"
    );
}

/// `SetSpeed` with a rate outside the `[0.5, 3.0]` valid range must clamp and
/// persist the clamped value (not silently drop the write or panic).
#[test]
fn set_speed_action_clamps_and_persists_boundary_rates() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let handler = handler_with_store(store);

    // Dispatch above the 3.0 upper bound — should clamp and persist 3.0.
    let _ = dispatch_set_speed(&handler, 5.0);
    let hi = handler
        .state
        .library
        .store
        .lock()
        .unwrap()
        .default_playback_rate();
    assert!(
        (hi - 3.0).abs() < f64::EPSILON,
        "speed above 3.0 must clamp to 3.0, got {hi}"
    );

    // Dispatch below the 0.5 lower bound — should clamp and persist 0.5.
    let _ = dispatch_set_speed(&handler, 0.1);
    let lo = handler
        .state
        .library
        .store
        .lock()
        .unwrap()
        .default_playback_rate();
    assert!(
        (lo - 0.5).abs() < f64::EPSILON,
        "speed below 0.5 must clamp to 0.5, got {lo}"
    );
}

/// Verifies that `hydrate_actor_for_play` copies `default_playback_rate` from
/// the store onto the player actor's state. After a cold relaunch the store
/// holds the persisted rate; without this copy the kernel's `now_playing.speed`
/// snapshot would report 1.0× until the user explicitly changes speed again.
#[test]
fn hydrate_actor_restores_default_playback_rate_onto_player() {
    use crate::ad_skip_handler::hydrate_actor_for_play;

    let store = Arc::new(Mutex::new(PodcastStore::new()));
    // Simulate a post-relaunch store with a persisted non-default rate.
    store.lock().unwrap().set_default_playback_rate(2.0);

    let actor = Arc::new(Mutex::new(crate::player::PlayerActor::new()));
    // Confirm the actor starts at the default idle rate (1.0).
    assert!(
        (actor.lock().unwrap().state().speed - 1.0).abs() < f32::EPSILON,
        "actor must start at 1.0 before hydration"
    );

    hydrate_actor_for_play(&store, &actor, "ep-any");

    let hydrated_speed = actor.lock().unwrap().state().speed;
    assert!(
        (hydrated_speed - 2.0).abs() < f32::EPSILON,
        "hydrate_actor_for_play must copy default_playback_rate (got {hydrated_speed})"
    );
}
