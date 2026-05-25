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

use super::handle::PodcastHandle;
use crate::capability::VoiceReport;
use crate::voice_handler::apply_report;

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

    let report_str = match unsafe { CStr::from_ptr(report_json) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let report: VoiceReport = match serde_json::from_str(report_str) {
        Ok(r) => r,
        Err(_) => return std::ptr::null_mut(),
    };

    let handle_ref = unsafe { &*handle };
    let changed = match handle_ref.voice_state.lock() {
        Ok(mut state) => apply_report(&mut state, report),
        Err(_) => false,
    };
    if changed {
        handle_ref
            .rev
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    std::ptr::null_mut()
}

#[cfg(test)]
mod tests {
    // Integration tests that exercise the full FFI round-trip would go
    // here. The pure-Rust projection unit tests live alongside the
    // `apply_report` impl in `crate::voice_handler::tests`.
}
