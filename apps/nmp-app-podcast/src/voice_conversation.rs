//! Voice conversation manager (M5.6-voice) — closes the STT→LLM→TTS loop.
//!
//! The voice capability already streams speech-to-text transcripts from
//! iOS into Rust (via `nmp_app_podcast_voice_report` →
//! [`crate::voice_handler::apply_report`]) and plays text-to-speech back
//! out (via [`crate::capability::VoiceCommand::Speak`]). The missing link
//! was the middle: when the user finishes speaking
//! ([`crate::capability::VoiceReport::TranscriptFinal`]) nothing ran an
//! LLM over the transcript — the kernel only surfaced the raw text under
//! the orb.
//!
//! This module supplies that link. [`VoiceConversationManager`] holds the
//! rolling turn history and, on each final transcript, spawns a
//! background LLM turn (reusing [`crate::agent_llm::chat_with_tools`] so
//! the assistant can query the podcast library) and dispatches the reply
//! back to the iOS TTS engine as a [`VoiceCommand::Speak`].
//!
//! ## Layering
//!
//! * [`run_turn`] is the pure, testable core: history in, reply out, no
//!   FFI. It owns the empty-transcript no-op policy and the
//!   unconditional `(user, assistant)` history append so the conversation
//!   accumulates even when Ollama is unreachable.
//! * [`VoiceConversationManager`] is the orchestrator: it owns the
//!   `*mut NmpApp` pointer, spawns [`run_turn`] off the actor thread, and
//!   dispatches the resulting `Speak`. The app pointer lives only here,
//!   never in [`run_turn`], so unit tests can exercise the conversation
//!   core without constructing an `NmpApp`.
//!
//! ## Doctrine
//!
//! * **D6** — every path degrades silently. Lock poison, LLM failure, or
//!   a dispatch encode error never panics across the FFI; the worst case
//!   is a missed turn.
//! * **D7** — the kernel decides what to speak. iOS reports the raw
//!   transcript; Rust runs the model and hands back the exact `Speak`
//!   text.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nmp_core::substrate::CapabilityRequest;
use nmp_ffi::NmpApp;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::agent_handler::SCAFFOLD_ASSISTANT_REPLY;
use crate::agent_llm;
use crate::capability::voice::{TtsProvider, VoiceCommand, VOICE_CAPABILITY_NAMESPACE};
use crate::ffi::projections::VoiceState;
use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::store::PodcastStore;
use crate::voice_handler;

/// System prompt for voice-mode turns. Kept terse on purpose: TTS replies
/// that run long are a poor voice UX, so we bias the model toward 1–3
/// conversational sentences while still granting library access via the
/// agent tool surface folded in by [`agent_llm::chat_with_tools`].
pub(crate) const VOICE_SYSTEM_PROMPT: &str = "You are a podcast assistant in voice mode. \
Give concise, conversational responses (1-3 sentences). You have access to the podcast \
library to answer questions.";

/// Shared rolling `(role, content)` turn history for the voice session.
pub(crate) type ConversationHistory = Arc<Mutex<Vec<(String, String)>>>;

/// Pure conversation core — no FFI, fully unit-testable.
///
/// Given the rolling `history`, the freshly-recognized `transcript`, and
/// the shared [`PodcastStore`] + Tokio [`Runtime`], this:
///
/// 1. **No-ops** on an empty / whitespace-only transcript: returns `None`
///    and leaves `history` untouched (the user didn't actually say
///    anything actionable).
/// 2. Pushes `("user", transcript)` onto `history`.
/// 3. Runs [`agent_llm::chat_with_tools`] against the prior turns.
/// 4. Pushes `("assistant", reply)` onto `history` **unconditionally** —
///    the real reply on success, [`SCAFFOLD_ASSISTANT_REPLY`] when the
///    model is unreachable — so the transcript stays a clean alternating
///    user/assistant sequence regardless of model availability.
/// 5. Returns `Some(reply)` for the caller to speak.
///
/// Returns `None` only for the empty-transcript no-op; every non-empty
/// transcript yields a speakable reply (possibly the fallback).
///
/// # Runtime
///
/// [`agent_llm::chat_with_tools`] calls [`Runtime::block_on`] internally,
/// so this function must run on a thread that is **not** already inside a
/// Tokio runtime. The production caller wraps it in
/// [`tokio::task::spawn_blocking`]; unit tests call it from the test's own
/// (non-async) thread.
pub(crate) fn run_turn(
    history: &ConversationHistory,
    transcript: &str,
    store: &Arc<Mutex<PodcastStore>>,
    runtime: &Runtime,
) -> Option<String> {
    if transcript.trim().is_empty() {
        return None;
    }

    // Snapshot prior turns (before this user turn) for the model, then
    // record the user turn. Holding the lock only for the clone keeps the
    // history mutex off the LLM round-trip.
    let prior: Vec<(String, String)> = match history.lock() {
        Ok(mut h) => {
            let snapshot = h.clone();
            h.push(("user".to_owned(), transcript.to_owned()));
            snapshot
        }
        // Poisoned history: degrade to a stateless single-turn call rather
        // than dropping the user entirely.
        Err(_) => Vec::new(),
    };

    let reply = agent_llm::chat_with_tools(
        VOICE_SYSTEM_PROMPT,
        &prior,
        transcript,
        Arc::clone(store),
        runtime,
    )
    .unwrap_or_else(|_| SCAFFOLD_ASSISTANT_REPLY.to_owned());

    if let Ok(mut h) = history.lock() {
        h.push(("assistant".to_owned(), reply.clone()));
    }

    Some(reply)
}

/// Orchestrates voice turns end-to-end: spawns [`run_turn`] off the actor
/// thread and dispatches the reply to the iOS TTS engine.
///
/// Owns the `*mut NmpApp` pointer (so the spawned task can issue
/// [`VoiceCommand::Speak`]) plus the shared history, store, voice-state,
/// runtime, and `rev` slots from the [`crate::ffi::PodcastHandle`].
///
/// ## Teardown fence (UAF fix)
///
/// The spawned turn dereferences `app` on a Tokio worker thread, and
/// `NmpApp::Drop` (nmp-ffi rev ec15ede) joins only the actor and
/// update-listener threads — it does NOT await this crate's runtime, and
/// `Runtime::drop` does not wait for detached `spawn`/`spawn_blocking`
/// tasks. Routing the dispatch back through the actor thread (the BACKLOG's
/// original suggestion) is not reachable: nmp-ffi exposes no accessor to
/// clone the capability-callback slot and no seam to post a closure onto the
/// actor thread, and we must not fork the pinned dep. Instead the manager
/// retains the outer task [`JoinHandle`]s in `inflight` and exposes
/// [`Self::shutdown`], which aborts and joins them. The FFI teardown calls
/// `shutdown` from [`crate::ffi::nmp_app_podcast_unregister`] —
/// contractually invoked *before* `nmp_app_free` — so every in-flight `app`
/// dereference is fenced before the allocation is freed. (`shutdown` cannot
/// live in a `Drop` impl: the snapshot-projection closure holds a second
/// strong `Arc<PodcastHandle>`, so the manager actually drops *during*
/// `nmp_app_free`, after the actor join, which is too late.)
pub(crate) struct VoiceConversationManager {
    app: *mut NmpApp,
    history: ConversationHistory,
    store: Arc<Mutex<PodcastStore>>,
    voice_state: Arc<Mutex<VoiceState>>,
    runtime: Arc<Runtime>,
    rev: Arc<AtomicU64>,
    snapshot_signal: Option<SnapshotUpdateSignal>,
    /// Voice-domain revision counter (from `DomainRevs::voice`).  Bumped on
    /// every async reply so the push-projection gate (`current == prev → None`)
    /// passes and the `podcast.voice` sidecar is actually emitted.
    domain_rev: Arc<AtomicU64>,
    /// Outer-task join handles for in-flight turns. Drained by
    /// [`Self::shutdown`] (abort + join) so no spawned task can dereference
    /// `app` after the app begins freeing. Pruned opportunistically on each
    /// new turn so the vector does not grow unbounded across a session.
    inflight: Arc<Mutex<Vec<JoinHandle<()>>>>,
    /// Set by [`Self::shutdown`]. A late `on_transcript_final` (which the
    /// caller contract already forbids after `unregister`) becomes a no-op
    /// rather than spawning a task that would dereference a freeing `app`.
    shutting_down: Arc<AtomicBool>,
}

// SAFETY: the `*mut NmpApp` is only ever *read*, never mutated — matching
// `PodcastHostOpHandler`. Unlike that handler (which dispatches
// synchronously on the actor / FFI thread, fenced by the actor
// join), this manager dereferences `app` from inside a `runtime.spawn` task
// on a Tokio worker thread. That deref is fenced instead by
// [`VoiceConversationManager::shutdown`], called from
// `nmp_app_podcast_unregister` (contractually before `nmp_app_free`): it
// aborts every in-flight outer task and joins it, so by the time the app is
// freed no task is between the `shutting_down` check and the `&*app` read.
// Aborting the outer future at its `.await` point drops the inner
// `spawn_blocking` handle without waiting on the LLM round-trip, so teardown
// stays prompt; the detached blocking thread only ever touches `Arc`-shared
// state (`run_turn` is FFI-free), never `app`.
unsafe impl Send for VoiceConversationManager {}
unsafe impl Sync for VoiceConversationManager {}

impl VoiceConversationManager {
    pub(crate) fn new(
        app: *mut NmpApp,
        history: ConversationHistory,
        store: Arc<Mutex<PodcastStore>>,
        voice_state: Arc<Mutex<VoiceState>>,
        runtime: Arc<Runtime>,
        rev: Arc<AtomicU64>,
        snapshot_signal: Option<SnapshotUpdateSignal>,
        domain_rev: Arc<AtomicU64>,
    ) -> Self {
        Self {
            app,
            history,
            store,
            voice_state,
            runtime,
            rev,
            snapshot_signal,
            domain_rev,
            inflight: Arc::new(Mutex::new(Vec::new())),
            shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Fence in-flight turns before the owning `NmpApp` frees.
    ///
    /// Sets `shutting_down` (so a late `on_transcript_final` no-ops) then
    /// aborts and joins every retained outer-task handle. Aborting cancels a
    /// task at its `.await` point before it can reach the `&*app` deref; a
    /// task already past the abort point completes its short dispatch and the
    /// join waits the few microseconds for it. Either way, when `shutdown`
    /// returns no spawned task will dereference `app` — so it is sound for
    /// `nmp_app_podcast_unregister` to call this immediately before
    /// `nmp_app_free`.
    ///
    /// Joins run via [`Runtime::block_on`]; `shutdown` therefore must be
    /// called from a thread that is NOT inside this runtime (the FFI/Swift
    /// thread that runs `unregister` qualifies).
    pub(crate) fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        let handles: Vec<JoinHandle<()>> = match self.inflight.lock() {
            Ok(mut g) => std::mem::take(&mut *g),
            Err(_) => return,
        };
        for handle in &handles {
            handle.abort();
        }
        // Aborting drops each outer future (and its inner `spawn_blocking`
        // handle), so these joins resolve promptly — no LLM round-trip wait.
        self.runtime.block_on(async {
            for handle in handles {
                let _ = handle.await;
            }
        });
    }

    /// Handle a final transcript from the STT engine — the user finished
    /// speaking. No-ops on an empty transcript (the empty-check is also
    /// enforced in [`run_turn`], but short-circuiting here avoids spawning
    /// a task that would do nothing). Otherwise spawns the LLM turn and,
    /// when the reply arrives, dispatches a [`VoiceCommand::Speak`] back to
    /// iOS and bumps `rev` so the next snapshot surfaces the assistant
    /// utterance.
    pub(crate) fn on_transcript_final(&self, transcript: String) {
        if transcript.trim().is_empty() {
            return;
        }
        // Teardown has begun (or completed) — never spawn a task that would
        // dereference a freeing `app`. The caller contract already forbids
        // host-op traffic after `unregister`; this is cheap insurance.
        if self.shutting_down.load(Ordering::SeqCst) {
            return;
        }

        let history = Arc::clone(&self.history);
        let store = Arc::clone(&self.store);
        let voice_state = Arc::clone(&self.voice_state);
        let runtime_for_blocking = Arc::clone(&self.runtime);
        let rev = Arc::clone(&self.rev);
        let snapshot_signal = self.snapshot_signal.clone();
        let domain_rev = Arc::clone(&self.domain_rev);
        let shutting_down = Arc::clone(&self.shutting_down);
        // `*mut NmpApp` is not `Send`; move it through a `usize` so the
        // spawned future captures a plain integer and re-materializes the
        // pointer on the other side. The `shutdown` fence (see the type-level
        // SAFETY note) guarantees the allocation outlives any deref the task
        // performs.
        let app_addr = self.app as usize;

        // Extra Arc clone so provider resolution (after spawn_blocking) can
        // access the store independently of the clone moved into the closure.
        let store_for_provider = Arc::clone(&self.store);

        let handle = self.runtime.spawn(async move {
            // `chat_with_tools` blocks on its own runtime internally, so it
            // must not run inside this async task directly. Offload to the
            // blocking pool (mirrors the other LLM handlers).
            let reply = tokio::task::spawn_blocking(move || {
                run_turn(&history, &transcript, &store, &runtime_for_blocking)
            })
            .await
            .ok()
            .flatten();

            let Some(reply) = reply else {
                return;
            };

            let request_id = format!("voice-{}", rev.load(Ordering::Relaxed));

            // Resolve TTS provider via the canonical helper (deduplicates the
            // ElevenLabs-vs-AvSpeech selection from voice_handler).
            let provider =
                voice_handler::resolve_tts_provider(&store_for_provider, &voice_state, None);

            // Update voice_state with the resolved voice id for UI feedback.
            if let Ok(mut v) = voice_state.lock() {
                v.is_speaking = true;
                v.current_request_id = Some(request_id.clone());
                v.last_response = Some(reply.clone());
                if let TtsProvider::ElevenLabs { voice_id: ref id, .. } = provider {
                    v.current_voice_id = Some(id.clone());
                }
            }

            let cmd = VoiceCommand::Speak {
                text: reply,
                request_id,
                provider,
            };
            if let Ok(payload_json) = serde_json::to_string(&cmd) {
                let req = CapabilityRequest {
                    namespace: VOICE_CAPABILITY_NAMESPACE.to_owned(),
                    correlation_id: String::new(),
                    payload_json,
                };
                // Final fence before the deref: bail if teardown has begun.
                // `shutdown` sets this flag and then aborts/joins this task;
                // there is no `.await` between this check and the `&*app`
                // read, so once the check passes the task runs the short
                // dispatch to completion and `shutdown`'s join waits for it
                // before `nmp_app_free`. If the flag is set, the deref is
                // skipped and the (possibly already-freeing) `app` is never
                // touched.
                if !shutting_down.load(Ordering::SeqCst) {
                    let app = app_addr as *mut NmpApp;
                    // SAFETY: see the type-level SAFETY note — the `shutdown`
                    // fence (called from `nmp_app_podcast_unregister`, before
                    // `nmp_app_free`) guarantees `app` is still live here.
                    let _ = unsafe { &*app }.dispatch_capability(&req);
                }
            }
            // Always advance the voice domain rev so the push-projection gate
            // (`current == prev → None`) passes and the `podcast.voice` sidecar
            // is emitted for this reply.
            domain_rev.fetch_add(1, Ordering::Relaxed);
            if let Some(signal) = snapshot_signal {
                signal.bump();
            } else {
                rev.fetch_add(1, Ordering::Relaxed);
            }
        });

        // Retain the outer handle so `shutdown` can abort/join it, and prune
        // already-finished turns so the vector doesn't grow across a session.
        if let Ok(mut g) = self.inflight.lock() {
            g.retain(|h| !h.is_finished());
            g.push(handle);
        }
    }
}

#[cfg(test)]
#[path = "voice_conversation_tests.rs"]
mod tests;
