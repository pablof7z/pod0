//! `nmp_app_podcast_voice_report` — async iOS→Rust voice-report channel.
//!
//! The iOS `VoiceCapability` fires this FFI entry point whenever the
//! `SFSpeechRecognizer` / `AVSpeechSynthesizer` runtime has a fresh
//! [`crate::capability::VoiceReport`] to deliver (partial transcript,
//! final transcript, listening started/stopped, speak started/finished,
//! error). Rust projects the report into the shared `voice_state` slot
//! on the [`super::handle::PodcastHandle`] so the next snapshot tick
//! surfaces the change.
//!
//! Modelled directly on [`super::audio_report::nmp_app_podcast_audio_report`].
//! Unlike audio, there is no synchronous follow-up `VoiceCommand` to
//! execute — voice mode is a pure observation channel for now. Future
//! milestones (real-LLM backend, barge-in policy in Rust) may return a
//! follow-up command; the signature already accommodates that.
//!
//! ## D6 — degrade silently
//!
//! Null pointers, invalid UTF-8, lock poison, and decode failures all
//! return `NULL` (treated by iOS as "no follow-up command"). Nothing
//! panics across the FFI.

use std::ffi::{c_char, CStr};
use std::sync::atomic::Ordering;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use crate::capability::{TtsProvider, VoiceCommand, VoiceReport, VOICE_CAPABILITY_NAMESPACE};
use crate::voice_handler::{apply_report, barge_in_text};

/// Deliver a JSON-encoded [`VoiceReport`] to the kernel-side voice
/// projection. Returns `NULL` — voice mode currently has no
/// synchronous follow-up command surface (the signature returns
/// `*mut c_char` so the bridge mirrors `nmp_app_podcast_audio_report`
/// and future milestones can plug a follow-up in without an ABI break).
///
/// Fire-and-forget: every failure mode (null pointer, bad UTF-8, decode
/// failure, lock poison) silently returns `NULL`.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_voice_report(
    handle: *mut PodcastHandle,
    report_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || report_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard("nmp_app_podcast_voice_report", std::ptr::null_mut, || {
        let report_str = match unsafe { CStr::from_ptr(report_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        let report: VoiceReport = match serde_json::from_str(report_str) {
            Ok(r) => r,
            Err(_) => return std::ptr::null_mut(),
        };

        let handle_ref = unsafe { &*handle };

        // Capture the final transcript text before apply_report consumes the
        // report (the LLM turn needs it after the lock is released).
        let final_transcript = match &report {
            VoiceReport::TranscriptFinal { text } => Some(text.clone()),
            _ => None,
        };

        // Capture failed request id for ElevenLabs fallback matching.
        let failed_request_id = match &report {
            VoiceReport::Failed { request_id, .. } => Some(request_id.clone()),
            _ => None,
        };

        // Barge-in fires only on non-empty/non-whitespace partial transcripts
        // (D7 — empty partials are STT noise; Rust owns barge-in policy).
        // Delegate to the canonical helper in voice_handler.
        let should_barge_in = barge_in_text(&report).is_some();

        // Apply the report and capture was_speaking + ElevenLabs fallback info
        // in a single lock window. Fallback text is read BEFORE apply_report
        // clears the tracking fields.
        let (changed, was_speaking, el_fallback_text) =
            match handle_ref.state.voice.voice_state.lock() {
                Ok(mut state) => {
                    let was_speaking = state.is_speaking;
                    // Snapshot ElevenLabs fallback text before apply_report clears it.
                    let el_fallback_text = if state.current_is_elevenlabs
                        && failed_request_id.as_deref()
                            == state.current_request_id.as_deref()
                    {
                        state.current_speak_text.clone()
                    } else {
                        None
                    };
                    let changed = apply_report(&mut state, report);
                    (changed, was_speaking, el_fallback_text)
                }
                Err(_) => (false, false, None),
            };
        handle_ref.bump_snapshot_rev_if(changed);

        // ElevenLabs fallback: on a Failed report for the in-flight ElevenLabs
        // Speak, retry with AvSpeech so the turn is still spoken.
        // `current_is_elevenlabs` is already cleared by apply_report (D7 —
        // no second retry on the AvSpeech fallback).
        if let Some(speak_text) = el_fallback_text {
            let fallback_request_id = format!(
                "fallback-{}",
                handle_ref.state.infra.rev.load(Ordering::Relaxed)
            );
            let cmd = VoiceCommand::Speak {
                text: speak_text,
                request_id: fallback_request_id,
                provider: TtsProvider::AvSpeech { voice_id: None },
            };
            if let Ok(payload_json) = serde_json::to_string(&cmd) {
                let req = nmp_core::substrate::CapabilityRequest {
                    namespace: VOICE_CAPABILITY_NAMESPACE.to_owned(),
                    correlation_id: String::new(),
                    payload_json,
                };
                // SAFETY: `handle_ref.app` is owned by `NmpApp` whose lifetime
                // brackets every report call (fenced by the actor join in Drop).
                let _ = unsafe { &*handle_ref.app }.dispatch_capability(&req);
            }
        }

        // Barge-in: non-empty partial transcript while TTS was speaking →
        // Stop the in-flight utterance so the user's voice wins the turn.
        if should_barge_in && was_speaking {
            if let Ok(payload_json) = serde_json::to_string(&VoiceCommand::Stop) {
                let req = nmp_core::substrate::CapabilityRequest {
                    namespace: VOICE_CAPABILITY_NAMESPACE.to_owned(),
                    correlation_id: String::new(),
                    payload_json,
                };
                // SAFETY: `handle_ref.app` is owned by `NmpApp` whose lifetime
                // brackets every report call (fenced by the actor join in Drop).
                let _ = unsafe { &*handle_ref.app }.dispatch_capability(&req);
            }
        }

        // Close the STT→LLM→TTS loop on a final transcript.
        if let Some(transcript) = final_transcript {
            handle_ref
                .state
                .voice
                .voice_conversation
                .on_transcript_final(transcript);
        }

        std::ptr::null_mut()
    })
}

#[cfg(test)]
#[path = "voice_report_tests.rs"]
mod tests;
