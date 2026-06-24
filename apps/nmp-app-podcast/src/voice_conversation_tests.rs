//! Tests for the voice conversation core ([`run_turn`]).
//!
//! These target the FFI-free core directly so they need no `NmpApp`. The
//! production [`VoiceConversationManager::on_transcript_final`] wraps
//! exactly this function in `tokio::task::spawn_blocking`, so testing the
//! core synchronously honours the milestone's intent (user/assistant turn
//! accumulation) without depending on the background-task plumbing.
//!
//! No live Ollama is required: `chat_with_tools` targets
//! `localhost:11434`, which fails fast with connection-refused when no
//! model server is running, so `run_turn` deterministically takes its
//! `SCAFFOLD_ASSISTANT_REPLY` fallback path and still appends the
//! assistant turn.

use super::*;
use crate::store::PodcastStore;

fn fixtures() -> (ConversationHistory, Arc<Mutex<PodcastStore>>, Runtime) {
    let history: ConversationHistory = Arc::new(Mutex::new(Vec::new()));
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    (history, store, runtime)
}

/// Build a manager with a NULL `app` pointer. Safe to drive only on paths
/// that never reach the `&*app` deref — i.e. after `shutdown()` has set the
/// `shutting_down` fence, which is exactly the teardown property under test.
fn manager_with_null_app() -> VoiceConversationManager {
    let history: ConversationHistory = Arc::new(Mutex::new(Vec::new()));
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let voice_state = Arc::new(Mutex::new(VoiceState::default()));
    let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
    let rev = Arc::new(AtomicU64::new(0));
    VoiceConversationManager::new(
        std::ptr::null_mut(),
        history,
        store,
        voice_state,
        runtime,
        rev,
        None,
        Arc::new(AtomicU64::new(0)),
    )
}

#[test]
fn voice_finished_with_empty_transcript_is_noop() {
    let (history, store, runtime) = fixtures();

    let reply = run_turn(&history, "", &store, &runtime);
    assert!(reply.is_none(), "empty transcript must not produce a reply");
    assert!(
        history.lock().unwrap().is_empty(),
        "empty transcript must not push any turns"
    );
}

#[test]
fn voice_finished_with_whitespace_transcript_is_noop() {
    let (history, store, runtime) = fixtures();

    let reply = run_turn(&history, "   \n\t ", &store, &runtime);
    assert!(reply.is_none());
    assert!(history.lock().unwrap().is_empty());
}

#[test]
fn conversation_history_accumulates() {
    let (history, store, runtime) = fixtures();

    // Two successive final transcripts → history grows to u + a + u + a.
    let first = run_turn(&history, "what should I listen to?", &store, &runtime);
    assert!(
        first.is_some(),
        "non-empty transcript yields a speakable reply"
    );

    let second = run_turn(&history, "tell me more", &store, &runtime);
    assert!(second.is_some());

    let h = history.lock().unwrap();
    assert_eq!(h.len(), 4, "expected user+assistant for each of two turns");
    assert_eq!(h[0].0, "user");
    assert_eq!(h[0].1, "what should I listen to?");
    assert_eq!(h[1].0, "assistant");
    assert_eq!(h[2].0, "user");
    assert_eq!(h[2].1, "tell me more");
    assert_eq!(h[3].0, "assistant");
}

#[test]
fn assistant_turn_appended_even_when_model_unreachable() {
    // With no Ollama running, the reply is the scaffold fallback — but it
    // must still be recorded as the assistant turn so the transcript stays
    // a clean alternating sequence.
    let (history, store, runtime) = fixtures();

    let reply = run_turn(&history, "hello", &store, &runtime).expect("reply");
    let h = history.lock().unwrap();
    assert_eq!(h.len(), 2);
    assert_eq!(h[1].0, "assistant");
    assert_eq!(h[1].1, reply);
}

#[test]
fn shutdown_on_idle_manager_is_a_noop() {
    // No turns spawned: shutdown must return immediately without touching
    // the (null) app pointer and must be callable from a non-runtime thread.
    let mgr = manager_with_null_app();
    mgr.shutdown();
    assert!(
        mgr.inflight.lock().unwrap().is_empty(),
        "no in-flight handles after an idle shutdown"
    );
}

#[test]
fn shutdown_is_idempotent() {
    let mgr = manager_with_null_app();
    mgr.shutdown();
    // A second drain (e.g. a defensive double-unregister) must not panic or
    // attempt to dereference the freed app.
    mgr.shutdown();
    assert!(mgr.shutting_down.load(Ordering::SeqCst));
}

#[test]
fn on_transcript_final_after_shutdown_does_not_spawn() {
    // After the teardown fence is set, a late final transcript (which the
    // caller contract forbids, but which a racing iOS report could still
    // deliver) must NOT spawn a task that would dereference the freeing app.
    let mgr = manager_with_null_app();
    mgr.shutdown();
    mgr.on_transcript_final("hello after teardown".to_owned());
    assert!(
        mgr.inflight.lock().unwrap().is_empty(),
        "no task may be spawned once shutting_down is set"
    );
}

#[test]
fn empty_transcript_never_spawns() {
    // The empty/whitespace short-circuit must not retain a handle either.
    let mgr = manager_with_null_app();
    mgr.on_transcript_final("   ".to_owned());
    assert!(mgr.inflight.lock().unwrap().is_empty());
    // Clean up: fence before drop (no task was spawned, so this is a no-op
    // drain, but it keeps the teardown contract explicit in the test).
    mgr.shutdown();
}

#[test]
fn shutdown_aborts_genuinely_inflight_task() {
    // The four shutdown/spawn tests above all exercise the *pre-spawn* guard:
    // `shutting_down` is set before any task exists, so they prove the fast
    // path no-ops but never prove `shutdown` cancels a task already running on
    // a worker thread — the actual UAF window. This test closes that gap by
    // parking a genuine in-flight task on the manager's runtime and asserting
    // `shutdown` cancels it.
    //
    // `shutdown` does `std::mem::take` then `await`s (and drops) each handle,
    // so the handles are consumed and an emptied-`inflight` check alone is
    // vacuous — it passes even if `shutdown` did nothing. To observe the abort
    // we capture a `Drop` guard *inside* the task future: when `abort` cancels
    // the parked future, tokio drops it, flipping `dropped` to true. Drain +
    // drop-guard together prove `shutdown` actually aborted the task.
    use std::future::pending;

    struct AbortFlag(Arc<AtomicBool>);
    impl Drop for AbortFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let mgr = manager_with_null_app();
    let rt = Arc::clone(&mgr.runtime); // same runtime `shutdown` will join on

    let dropped = Arc::new(AtomicBool::new(false));
    let guard = AbortFlag(Arc::clone(&dropped));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = rt.spawn(async move {
        let _guard = guard; // lives as part of the future's state
        let _ = started_tx.send(()); // announce we are genuinely running
        pending::<()>().await; // park forever — only `abort` can end this
    });
    mgr.inflight.lock().unwrap().push(handle);

    // Prove the task is actually in-flight (parked, past its first poll)
    // before `shutdown` — this is the in-flight case, not the pre-spawn guard.
    // This `block_on` completes before `shutdown` runs its own `block_on`, so
    // there is no nested-runtime panic.
    rt.block_on(async {
        started_rx
            .await
            .expect("in-flight task must reach its park point");
    });

    mgr.shutdown();

    assert!(
        mgr.inflight.lock().unwrap().is_empty(),
        "shutdown must drain the inflight handles"
    );
    assert!(
        dropped.load(Ordering::SeqCst),
        "shutdown must abort (and thereby drop) the in-flight task future"
    );
}
