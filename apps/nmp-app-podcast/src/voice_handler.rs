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
use std::sync::{Arc, Mutex};

use nmp_core::substrate::CapabilityRequest;

use crate::capability::{TtsProvider, VoiceCommand, VoiceReport, VOICE_CAPABILITY_NAMESPACE};
use crate::ffi::actions::voice_module::VoiceAction;
use crate::ffi::projections::VoiceState;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::store::PodcastStore;

/// Resolve the TTS provider from the store's current settings.
///
/// If an ElevenLabs voice id is configured, returns `TtsProvider::ElevenLabs`;
/// otherwise returns `TtsProvider::AvSpeech` with the resolved voice id.
/// For AvSpeech, `explicit_voice_id` takes priority; if absent or empty,
/// falls back to the `current_voice_id` stored in `VoiceState` so the
/// user's last selected on-device voice is preserved across turns.
pub(crate) fn resolve_tts_provider(
    store: &Arc<Mutex<PodcastStore>>,
    voice_state: &Arc<Mutex<VoiceState>>,
    explicit_voice_id: Option<String>,
) -> TtsProvider {
    let (el_voice_id, el_model) = store
        .lock()
        .ok()
        .map(|s| {
            let vid = s.eleven_labs_voice_id().trim().to_owned();
            let model = s.eleven_labs_tts_model().trim().to_owned();
            let model = if model.is_empty() { None } else { Some(model) };
            (vid, model)
        })
        .unwrap_or_default();

    if !el_voice_id.is_empty() {
        TtsProvider::ElevenLabs {
            voice_id: el_voice_id,
            model: el_model,
        }
    } else {
        // If no explicit voice_id, fall back to the stored current_voice_id
        // from VoiceState so the user's on-device voice preference is
        // preserved across turns (Swift no longer holds activeVoiceID).
        let resolved = explicit_voice_id
            .filter(|v| !v.is_empty())
            .or_else(|| {
                voice_state
                    .lock()
                    .ok()
                    .and_then(|v| v.current_voice_id.clone())
            });
        TtsProvider::AvSpeech { voice_id: resolved }
    }
}

/// Whether a partial transcript report should trigger a barge-in Stop.
/// Returns the non-empty trimmed text if barge-in should fire, or `None` if not.
/// Empty or whitespace-only partial transcripts are noise; barge-in must not
/// cancel an in-flight utterance for those.
pub(crate) fn barge_in_text(report: &VoiceReport) -> Option<&str> {
    match report {
        VoiceReport::TranscriptPartial { text } if !text.trim().is_empty() => Some(text.trim()),
        _ => None,
    }
}

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
            let request_id = format!("turn-{}", handler.state.infra.rev.load(Ordering::Relaxed));
            // Resolve provider from store: if an ElevenLabs voice is configured,
            // use it; otherwise fall back to on-device AVSpeech.
            let voice_state_arc = handler.state.voice.voice_state.share();
            let provider =
                resolve_tts_provider(&handler.state.library.store, &voice_state_arc, voice_id);
            mutate_voice_state(handler, |v| {
                v.is_speaking = true;
                v.current_request_id = Some(request_id.clone());
                v.current_speak_text = Some(text.clone());
                v.current_is_elevenlabs = matches!(provider, TtsProvider::ElevenLabs { .. });
                if let TtsProvider::ElevenLabs { voice_id: ref id, .. } = provider {
                    v.current_voice_id = Some(id.clone());
                } else if let TtsProvider::AvSpeech { voice_id: Some(ref id) } = provider {
                    v.current_voice_id = Some(id.clone());
                }
                v.last_response = Some(text.clone());
            });
            let cmd = VoiceCommand::Speak {
                text,
                request_id,
                provider,
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
                v.current_speak_text = None;
                v.current_is_elevenlabs = false;
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
            state.current_speak_text = None;
            state.current_is_elevenlabs = false;
        }
        VoiceReport::Failed { error, .. } => {
            state.is_speaking = false;
            state.current_request_id = None;
            // Clear tracking fields after capturing them for the fallback
            // (voice_report.rs reads these before calling apply_report).
            state.current_speak_text = None;
            state.current_is_elevenlabs = false;
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
/// `pub(crate)` so that domain-projection tests in `ffi` can drive voice
/// state mutations without going through the full FFI boundary.
pub(crate) fn mutate_voice_state(handler: &PodcastHostOpHandler, f: impl FnOnce(&mut VoiceState)) {
    // Step 12: voice_state now lives in state.voice (VoiceSubstate).
    if let Ok(mut v) = handler.state.voice.voice_state.lock() {
        f(&mut v);
    }
    // bump() routes to Domain::Voice → advances domain_revs.voice so the
    // podcast.voice push sidecar emits on the next frame.
    handler.state.voice.infra.bump();
}

#[cfg(test)]
#[path = "voice_handler_tests.rs"]
mod tests;
