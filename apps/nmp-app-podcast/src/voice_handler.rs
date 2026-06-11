//! Voice action + report handler — kernel-side wiring for the
//! `nmp.voice.capability` executor (feature #42).
//!
//! Split out of [`crate::host_op_handler`] to keep that module under the
//! 500-LOC hard ceiling once voice-mode dispatch landed. The two public
//! entry points are:
//!
//! * [`handle`] — invoked by `PodcastHostOpHandler` when a
//!   `podcast.voice.*` action arrives. Mutates the shared `voice_state`
//!   slot on the handle and dispatches the corresponding
//!   [`crate::capability::VoiceCommand`] to the iOS executor.
//! * [`apply_report`] — invoked by `nmp_app_podcast_voice_report` when
//!   the iOS executor reports an STT/TTS event back to Rust. Projects
//!   the report into the shared `voice_state` slot so the next snapshot
//!   tick surfaces it.
//!
//! ## Doctrine
//!
//! * **D6** — every helper degrades silently on lock poisoning,
//!   serialization failure, or unknown variants. Nothing panics across
//!   the FFI; the worst-case is a missed projection update.
//! * **D7** — the kernel decides *what* state to surface (e.g. clearing
//!   `partial_transcript` on a final transcript); iOS only reports the
//!   raw event.

use std::sync::atomic::Ordering;

use nmp_core::substrate::CapabilityRequest;

use crate::capability::{VoiceCommand, VoiceReport, VOICE_CAPABILITY_NAMESPACE};
use crate::ffi::actions::voice_module::VoiceAction;
use crate::ffi::projections::VoiceState;
use crate::host_op_handler::PodcastHostOpHandler;

/// Dispatch a typed [`VoiceCommand`] to the iOS voice executor. Returns
/// `Err(message)` on JSON encode failure; the capability call itself is
/// fire-and-forget — late results arrive asynchronously via
/// [`apply_report`].
fn dispatch_voice(
    handler: &PodcastHostOpHandler,
    cmd: &VoiceCommand,
    correlation_id: &str,
) -> Result<(), String> {
    let payload_json = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
    let req = CapabilityRequest {
        namespace: VOICE_CAPABILITY_NAMESPACE.to_owned(),
        correlation_id: correlation_id.to_owned(),
        payload_json,
    };
    // SAFETY: `handler.app` is owned by `NmpApp` whose lifetime brackets
    // every host-op call (drop fences the actor join before free).
    let _ = unsafe { &*handler.app }.dispatch_capability(&req);
    Ok(())
}

/// Apply a [`VoiceAction`] from the shell. Updates the kernel-side
/// `voice_state` projection optimistically (so the UI flips immediately)
/// and dispatches the matching [`VoiceCommand`] to the iOS executor.
/// Returns the JSON envelope `PodcastHostOpHandler::handle` returns.
pub(crate) fn handle(
    handler: &PodcastHostOpHandler,
    action: VoiceAction,
    correlation_id: &str,
) -> serde_json::Value {
    match action {
        VoiceAction::Activate => {
            mutate_voice_state(handler, |v| {
                v.is_listening = true;
                // Clear stale partial from the previous turn so the UI
                // doesn't render last session's caption while waiting
                // for the first new partial.
                v.partial_transcript = None;
            });
            match dispatch_voice(handler, &VoiceCommand::StartListening, correlation_id) {
                Ok(_) => serde_json::json!({"ok": true}),
                Err(e) => serde_json::json!({"ok": false, "error": e}),
            }
        }
        VoiceAction::Deactivate => {
            mutate_voice_state(handler, |v| {
                v.is_listening = false;
                v.partial_transcript = None;
            });
            match dispatch_voice(handler, &VoiceCommand::StopListening, correlation_id) {
                Ok(_) => serde_json::json!({"ok": true}),
                Err(e) => serde_json::json!({"ok": false, "error": e}),
            }
        }
        VoiceAction::Speak { text, voice_id } => {
            // Mint a kernel-owned request id so the executor's reports
            // correlate even when the UI didn't supply one.
            let request_id = format!("turn-{}", handler.rev.load(Ordering::Relaxed));
            mutate_voice_state(handler, |v| {
                v.is_speaking = true;
                v.current_request_id = Some(request_id.clone());
                if let Some(id) = voice_id.as_ref() {
                    v.current_voice_id = Some(id.clone());
                }
                // Surface the assistant utterance under the orb.
                v.last_response = Some(text.clone());
            });
            let cmd = VoiceCommand::Speak {
                text,
                voice_id,
                request_id,
            };
            match dispatch_voice(handler, &cmd, correlation_id) {
                Ok(_) => serde_json::json!({"ok": true}),
                Err(e) => serde_json::json!({"ok": false, "error": e}),
            }
        }
        VoiceAction::Stop => {
            mutate_voice_state(handler, |v| {
                v.is_speaking = false;
                v.current_request_id = None;
            });
            match dispatch_voice(handler, &VoiceCommand::Stop, correlation_id) {
                Ok(_) => serde_json::json!({"ok": true}),
                Err(e) => serde_json::json!({"ok": false, "error": e}),
            }
        }
        VoiceAction::SetVoice { voice_id } => {
            mutate_voice_state(handler, |v| {
                v.current_voice_id = Some(voice_id.clone());
            });
            let cmd = VoiceCommand::SetVoice { voice_id };
            match dispatch_voice(handler, &cmd, correlation_id) {
                Ok(_) => serde_json::json!({"ok": true}),
                Err(e) => serde_json::json!({"ok": false, "error": e}),
            }
        }
    }
}

/// Project a [`VoiceReport`] from the iOS executor into the kernel-side
/// `voice_state`. Returns `true` when the projection changed (so callers
/// can bump `rev`), `false` when the report was a no-op.
pub(crate) fn apply_report(state: &mut VoiceState, report: VoiceReport) -> bool {
    let before = state.clone();
    match report {
        VoiceReport::Started { request_id } => {
            state.is_speaking = true;
            state.current_request_id = Some(request_id);
        }
        VoiceReport::Finished { .. } | VoiceReport::Stopped => {
            state.is_speaking = false;
            state.current_request_id = None;
        }
        VoiceReport::Failed { error, .. } => {
            state.is_speaking = false;
            state.current_request_id = None;
            state.last_response = Some(format!("Voice error: {error}"));
        }
        VoiceReport::ListeningStarted => {
            state.is_listening = true;
        }
        VoiceReport::ListeningStopped => {
            state.is_listening = false;
            state.partial_transcript = None;
        }
        VoiceReport::TranscriptPartial { text } => {
            state.partial_transcript = Some(text);
        }
        VoiceReport::TranscriptFinal { text } => {
            state.partial_transcript = None;
            state.last_response = Some(text);
        }
        VoiceReport::Error { message } => {
            state.last_response = Some(format!("Voice error: {message}"));
        }
    }
    *state != before
}

/// Lock-and-mutate helper. Silently no-ops on lock poison (D6) and
/// bumps `rev` so the next snapshot tick surfaces the change.
fn mutate_voice_state(handler: &PodcastHostOpHandler, f: impl FnOnce(&mut VoiceState)) {
    // Step 12: voice_state now lives in state.voice (VoiceSubstate).
    if let Ok(mut v) = handler.state.voice.voice_state.lock() {
        f(&mut v);
    }
    handler.rev.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
#[path = "voice_handler_tests.rs"]
mod tests;
