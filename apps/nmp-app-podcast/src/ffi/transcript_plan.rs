//! `nmp_app_podcast_transcript_ingest_plan` — synchronous Rust-owned transcript
//! ingestion planner.
//!
//! Swift executes network/STT capabilities, but Rust owns the policy branch:
//! already-ready, per-podcast opt-out, publisher-first, AI fallback, provider
//! resolution, key gating, and Apple-native local-file gating.

use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, CStr, CString};

use podcast_core::TranscriptKind;
use serde::{Deserialize, Serialize};

use super::guard::ffi_guard;
use super::handle::PodcastHandle;

#[derive(Debug, Deserialize)]
struct TranscriptPlanRequest {
    episode_id: String,
    #[serde(default)]
    force_provider: Option<String>,
    #[serde(default)]
    local_audio_available: bool,
    #[serde(default = "default_true")]
    allow_publisher: bool,
    #[serde(default)]
    auto_ingest: bool,
}

#[derive(Debug, Deserialize)]
struct TranscriptAutoIngestRequest {
    #[serde(default = "default_max_count")]
    max_count: usize,
    #[serde(default)]
    episode_ids: Vec<String>,
    #[serde(default)]
    local_audio_available: Vec<LocalAudioAvailability>,
}

fn default_max_count() -> usize {
    5
}

#[derive(Debug, Deserialize)]
struct LocalAudioAvailability {
    episode_id: String,
    available: bool,
}

#[derive(Debug, Serialize)]
struct TranscriptAutoIngestResponse {
    ok: bool,
    episode_ids: Vec<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
struct TranscriptPlanResponse {
    ok: bool,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    publisher_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_hint: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    requires_local_file: bool,
}

fn kind_raw(kind: TranscriptKind) -> &'static str {
    match kind {
        TranscriptKind::Vtt => "text/vtt",
        TranscriptKind::Srt => "application/x-subrip",
        TranscriptKind::Json => "application/json",
        TranscriptKind::Html => "text/html",
        TranscriptKind::Text => "text/plain",
    }
}

fn skipped(reason: impl Into<String>) -> TranscriptPlanResponse {
    TranscriptPlanResponse {
        ok: true,
        status: "skipped",
        reason: Some(reason.into()),
        publisher_url: None,
        mime_hint: None,
        provider: None,
        requires_local_file: false,
    }
}

fn stt(provider: impl Into<String>) -> TranscriptPlanResponse {
    let provider = provider.into();
    let requires_local_file = provider == crate::store::stt_policy::APPLE_NATIVE;
    TranscriptPlanResponse {
        ok: true,
        status: "stt",
        reason: None,
        publisher_url: None,
        mime_hint: None,
        provider: Some(provider),
        requires_local_file,
    }
}

/// Plan transcript ingestion for one episode.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_transcript_ingest_plan(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_transcript_ingest_plan",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: TranscriptPlanRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return std::ptr::null_mut(),
            };
            let handle_ref = unsafe { &*handle };
            let response = match handle_ref.state.library.store.lock() {
                Ok(s) => plan_transcript_ingest(&s, &request),
                Err(_) => TranscriptPlanResponse {
                    ok: false,
                    status: "error",
                    reason: Some("store poisoned".to_owned()),
                    publisher_url: None,
                    mime_hint: None,
                    provider: None,
                    requires_local_file: false,
                },
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

/// Resolve the next auto-ingest candidates from canonical Rust library state.
///
/// Swift supplies only native capability facts (which episode ids currently
/// have local audio files). Rust owns candidate eligibility, publisher/STT
/// policy, newest-first ordering, optional new-episode scoping, and max count.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_transcript_auto_ingest_candidates(
    handle: *mut PodcastHandle,
    request_json: *const c_char,
) -> *mut c_char {
    if handle.is_null() || request_json.is_null() {
        return std::ptr::null_mut();
    }
    ffi_guard(
        "nmp_app_podcast_transcript_auto_ingest_candidates",
        std::ptr::null_mut,
        || {
            let request_str = match unsafe { CStr::from_ptr(request_json) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            let request: TranscriptAutoIngestRequest = match serde_json::from_str(request_str) {
                Ok(r) => r,
                Err(_) => return std::ptr::null_mut(),
            };
            let local_audio: HashMap<String, bool> = request
                .local_audio_available
                .into_iter()
                .map(|fact| (fact.episode_id.to_lowercase(), fact.available))
                .collect();
            let scoped: HashSet<String> = request
                .episode_ids
                .into_iter()
                .map(|id| id.to_lowercase())
                .collect();
            let handle_ref = unsafe { &*handle };
            let response = match handle_ref.state.library.store.lock() {
                Ok(s) => {
                    let mut candidates = Vec::new();
                    for (podcast, episodes) in s.all_podcasts() {
                        if !s.is_transcription_enabled(&podcast.id) {
                            continue;
                        }
                        for episode in episodes {
                            let episode_id = episode.id.0.to_string();
                            if !scoped.is_empty() && !scoped.contains(&episode_id) {
                                continue;
                            }
                            let request = TranscriptPlanRequest {
                                episode_id: episode_id.clone(),
                                force_provider: None,
                                local_audio_available: local_audio
                                    .get(&episode_id)
                                    .copied()
                                    .unwrap_or(false),
                                allow_publisher: true,
                                auto_ingest: true,
                            };
                            let plan = plan_transcript_ingest(&s, &request);
                            if plan.ok && matches!(plan.status, "publisher" | "stt") {
                                candidates.push((episode.pub_date, episode_id));
                            }
                        }
                    }
                    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                    TranscriptAutoIngestResponse {
                        ok: true,
                        episode_ids: candidates
                            .into_iter()
                            .take(request.max_count)
                            .map(|(_, id)| id)
                            .collect(),
                    }
                }
                Err(_) => TranscriptAutoIngestResponse {
                    ok: false,
                    episode_ids: Vec::new(),
                },
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

fn plan_stt(
    store: &crate::store::PodcastStore,
    request: &TranscriptPlanRequest,
) -> TranscriptPlanResponse {
    let provider = request
        .force_provider
        .clone()
        .unwrap_or_else(|| store.effective_stt_provider().to_owned());
    if request.force_provider.is_none() && !store.auto_fallback_to_scribe() {
        return skipped(
            "No publisher transcript, and automatic AI transcription is off (turn it on in Settings).",
        );
    }
    if crate::store::stt_policy::requires_key(&provider) && !store.stt_key_present(&provider) {
        return skipped(format!("No API key is configured for {provider}."));
    }
    if provider == crate::store::stt_policy::APPLE_NATIVE && !request.local_audio_available {
        return skipped(
            "On-device transcription needs the episode downloaded, but the audio file wasn't found.",
        );
    }
    stt(provider)
}

fn plan_transcript_ingest(
    store: &crate::store::PodcastStore,
    request: &TranscriptPlanRequest,
) -> TranscriptPlanResponse {
    let episode_id = request.episode_id.to_lowercase();
    if !store.has_episode(&episode_id) {
        return TranscriptPlanResponse {
            ok: false,
            status: "error",
            reason: Some(format!("episode not found: {}", request.episode_id)),
            publisher_url: None,
            mime_hint: None,
            provider: None,
            requires_local_file: false,
        };
    }
    if store
        .transcript_for(&episode_id)
        .map(|t| !t.trim().is_empty())
        .unwrap_or(false)
    {
        return TranscriptPlanResponse {
            ok: true,
            status: "ready",
            reason: None,
            publisher_url: None,
            mime_hint: None,
            provider: None,
            requires_local_file: false,
        };
    }
    let podcast_id = store.all_podcasts().into_iter().find_map(|(podcast, episodes)| {
        episodes
            .iter()
            .any(|ep| ep.id.0.to_string() == episode_id)
            .then_some(podcast.id)
    });
    let Some(pid) = podcast_id else {
        return TranscriptPlanResponse {
            ok: false,
            status: "error",
            reason: Some(format!("episode not found: {}", request.episode_id)),
            publisher_url: None,
            mime_hint: None,
            provider: None,
            requires_local_file: false,
        };
    };
    if !store.is_transcription_enabled(&pid) {
        return skipped("Transcription is turned off for this show's category.");
    }
    if request.force_provider.is_none() && request.allow_publisher {
        if let Some((url, kind)) = store.episode_publisher_transcript(&episode_id) {
            if !request.auto_ingest || store.auto_ingest_publisher_transcripts() {
                return TranscriptPlanResponse {
                    ok: true,
                    status: "publisher",
                    reason: None,
                    publisher_url: Some(url),
                    mime_hint: Some(kind_raw(kind)),
                    provider: None,
                    requires_local_file: false,
                };
            }
        }
    }
    plan_stt(store, request)
}
