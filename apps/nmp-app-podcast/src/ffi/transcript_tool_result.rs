//! `nmp_app_podcast_transcript_tool_result` — Rust-owned transcript tool
//! result summarizer.
//!
//! Swift executes host transcript capabilities, but agent-facing status
//! interpretation belongs here: whether an episode is ready, failed, still in
//! progress, unavailable, or unknown.

use std::ffi::{c_char, CStr, CString};

use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[derive(Debug, Deserialize)]
struct TranscriptToolResultRequest {
    episode_id: String,
}

#[derive(Debug, Serialize)]
struct TranscriptToolResultResponse {
    ok: bool,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl TranscriptToolResultResponse {
    fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            status: "error".to_owned(),
            source: None,
            message: Some(message.into()),
        }
    }

    fn ready() -> Self {
        Self {
            ok: true,
            status: "ready".to_owned(),
            source: None,
            message: Some("Transcript already available.".to_owned()),
        }
    }

    fn unavailable() -> Self {
        Self {
            ok: true,
            status: "unavailable".to_owned(),
            source: None,
            message: Some("Transcription could not complete. Check STT provider settings.".to_owned()),
        }
    }
}

/// Summarize the current transcript status for agent-tool result envelopes.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_transcript_tool_result(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_transcript_tool_result",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: TranscriptToolResultRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return std::ptr::null_mut(),
            };
            let episode_id = request.episode_id.to_lowercase();
            let handle_ref = unsafe { &*handle };
            let response = match handle_ref.state.library.store.lock() {
                Ok(s) => {
                    if !s.has_episode(&episode_id) {
                        TranscriptToolResultResponse::error(format!(
                            "episode not found: {}",
                            request.episode_id
                        ))
                    } else if s
                        .transcript_for(&episode_id)
                        .map(|t| !t.trim().is_empty())
                        .unwrap_or(false)
                    {
                        TranscriptToolResultResponse::ready()
                    } else if let Some((status, message)) = s.transcript_status_for(&episode_id) {
                        TranscriptToolResultResponse {
                            ok: true,
                            status: status.clone(),
                            source: None,
                            message: message.clone().or_else(|| match status.as_str() {
                                "failed" => Some("Transcription did not finish.".to_owned()),
                                "queued" => Some("Transcript ingestion is queued.".to_owned()),
                                "fetching_publisher" => Some("Publisher transcript fetch is in progress.".to_owned()),
                                "transcribing" => Some("Audio transcription is in progress.".to_owned()),
                                _ => None,
                            }),
                        }
                    } else {
                        TranscriptToolResultResponse::unavailable()
                    }
                }
                Err(_) => TranscriptToolResultResponse::error("store poisoned"),
            };
            match serde_json::to_string(&response) {
                Ok(json) => match CString::new(json) {
                    Ok(c) => c.into_raw(),
                    Err(_) => std::ptr::null_mut(),
                },
                Err(_) => std::ptr::null_mut(),
            }
        },
    )
}
