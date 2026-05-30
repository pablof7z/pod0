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

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nmp_core::substrate::CapabilityRequest;
use nmp_ffi::NmpApp;
use tokio::runtime::Runtime;

use crate::agent_handler::SCAFFOLD_ASSISTANT_REPLY;
use crate::agent_llm;
use crate::capability::voice::{VoiceCommand, VOICE_CAPABILITY_NAMESPACE};
use crate::ffi::projections::VoiceState;
use crate::store::PodcastStore;

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
pub(crate) struct VoiceConversationManager {
    app: *mut NmpApp,
    history: ConversationHistory,
    store: Arc<Mutex<PodcastStore>>,
    voice_state: Arc<Mutex<VoiceState>>,
    runtime: Arc<Runtime>,
    rev: Arc<AtomicU64>,
}

// SAFETY: the `*mut NmpApp` is only ever *read*, never mutated — that part
// matches `PodcastHostOpHandler` / `TtsEpisodeHandler`. BUT unlike those
// handlers (which dispatch synchronously on the actor / FFI thread), this
// manager dereferences `app` from inside a `runtime.spawn` task on a Tokio
// worker thread. `NmpApp::Drop` (nmp-ffi rev ec15ede) joins only the actor
// thread and the update-listener thread; it does NOT await this crate's
// Tokio runtime, and `Runtime::drop` does not wait for detached
// `spawn`/`spawn_blocking` tasks. So a long-running LLM turn that returns
// *after* `nmp_app_free` has begun is an unfenced read of a freeing
// `NmpApp` — a teardown race this `Send` impl does not currently close.
// In practice the window requires the user to tear down voice mode mid-turn
// AND the app to be freed within the few seconds of the turn; the loop is
// otherwise sound. The architecturally-correct fix (route the `Speak`
// dispatch back through the actor thread so the existing join fences it) is
// tracked in docs/BACKLOG.md ("voice-conversation off-thread dispatch UAF").
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
    ) -> Self {
        Self {
            app,
            history,
            store,
            voice_state,
            runtime,
            rev,
        }
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

        let history = Arc::clone(&self.history);
        let store = Arc::clone(&self.store);
        let voice_state = Arc::clone(&self.voice_state);
        let runtime_for_blocking = Arc::clone(&self.runtime);
        let rev = Arc::clone(&self.rev);
        // `*mut NmpApp` is not `Send`; move it through a `usize` so the
        // spawned future captures a plain integer and re-materializes the
        // pointer on the other side. The SAFETY contract above guarantees
        // the allocation outlives the task.
        let app_addr = self.app as usize;

        self.runtime.spawn(async move {
            // `chat_with_tools` blocks on its own runtime internally, so it
            // must not run inside this async task directly. Offload to the
            // blocking pool (mirrors `briefings_handler`).
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

            // Optimistically flip the orb to "speaking" and surface the
            // assistant utterance before the TTS `Started` report lands, so
            // the UI doesn't show an idle gap while audio spins up. The
            // `Started`/`Finished` reports self-correct this on arrival.
            if let Ok(mut v) = voice_state.lock() {
                v.is_speaking = true;
                v.current_request_id = Some(request_id.clone());
                v.last_response = Some(reply.clone());
            }

            let cmd = VoiceCommand::Speak {
                text: reply,
                voice_id: None,
                request_id,
            };
            if let Ok(payload_json) = serde_json::to_string(&cmd) {
                let req = CapabilityRequest {
                    namespace: VOICE_CAPABILITY_NAMESPACE.to_owned(),
                    correlation_id: String::new(),
                    payload_json,
                };
                let app = app_addr as *mut NmpApp;
                // SAFETY: see the `unsafe impl Send` note — `app` outlives
                // every spawned turn (runtime dropped after the actor join).
                let _ = unsafe { &*app }.dispatch_capability(&req);
            }
            rev.fetch_add(1, Ordering::Relaxed);
        });
    }
}

#[cfg(test)]
#[path = "voice_conversation_tests.rs"]
mod tests;
