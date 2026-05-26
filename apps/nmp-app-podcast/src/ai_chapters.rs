//! AI chapter compilation — synthesizes equal-length stub chapters from a
//! cached transcript when an episode has no RSS / Podcasting 2.0 chapters.
//!
//! Mirrors the legacy `App/Sources/Services/AIChapterCompiler.swift` in
//! shape (one LLM round-trip per episode) but the *implementation* is
//! deliberately a local stub for this PR: we don't call OpenRouter, we
//! just slice the episode duration into `STUB_CHAPTER_COUNT` evenly-
//! spaced segments labelled `"Chapter 1"`, `"Chapter 2"`, … and stamp
//! `is_ai_generated = true` on each `podcast_core::Chapter`. The plumbing
//! (action wire shape, store mutation, projection field, iOS button +
//! sparkles badge) is what we're shipping; the real LLM round-trip lands
//! in a follow-up that swaps the body of [`build_stub_chapters`] for a
//! `dispatch_http` call to OpenRouter and a JSON parse step.
//!
//! ## Design notes
//!
//! * **Gating.** Refuses to compile when the episode already has any
//!   chapters (RSS-supplied chapters always win — D7, kernel decides);
//!   when no transcript has been cached yet (the iOS shell is responsible
//!   for dispatching `podcast.fetch_transcript` first); and when the
//!   episode duration is unknown (we need it to compute segment offsets).
//! * **Idempotent.** A second `compile` call on an episode that already
//!   has AI-generated chapters is a no-op. To regenerate, the caller
//!   must dispatch a future `podcast.chapters.clear` first (out of scope).
//! * **D6.** Errors degrade silently through the `{"ok":false,"error":…}`
//!   envelope; the iOS shell renders the error toast.
//! * **D7.** The kernel decides the chapter count + naming scheme; the
//!   iOS shell only renders.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use podcast_core::Chapter;

use crate::store::PodcastStore;

/// Number of equally-spaced chapters the stub LLM emits.
///
/// Picked to land between [`MIN_CHAPTERS`] and [`MAX_CHAPTERS`] from the
/// legacy `AIChapterCompiler`. The real LLM round-trip will return between
/// 4 and 12 per the system prompt; the stub picks the midpoint so the UI
/// feedback (sparkles badge on five rows) is representative.
const STUB_CHAPTER_COUNT: usize = 5;

#[derive(Debug, PartialEq)]
pub(crate) enum CompileOutcome {
    /// New AI chapters were synthesized and persisted.
    Compiled { chapter_count: usize },
    /// Episode already has chapters (RSS or prior AI compile) — no-op.
    AlreadyHasChapters,
    /// No cached transcript — the caller must dispatch
    /// `podcast.fetch_transcript` first.
    NoTranscript,
    /// Episode duration is unknown so we can't compute segment offsets.
    NoDuration,
    /// Episode id didn't resolve to any episode in the store.
    EpisodeNotFound,
}

/// Public host-op entry point. Mirrors the shape of
/// [`crate::chapter::handle_fetch_chapters`] so the host-op handler can
/// dispatch both with the same call shape.
pub(crate) fn handle_compile_chapters(
    store: &Arc<Mutex<PodcastStore>>,
    rev: &AtomicU64,
    episode_id: String,
) -> serde_json::Value {
    match compile(store, &episode_id) {
        Ok(CompileOutcome::Compiled { chapter_count }) => {
            rev.fetch_add(1, Ordering::Relaxed);
            serde_json::json!({
                "ok": true,
                "status": "compiling",
                "chapter_count": chapter_count,
            })
        }
        Ok(CompileOutcome::AlreadyHasChapters) => {
            serde_json::json!({"ok": true, "status": "already_has_chapters"})
        }
        Ok(CompileOutcome::NoTranscript) => {
            serde_json::json!({"ok": false, "error": "no_transcript"})
        }
        Ok(CompileOutcome::NoDuration) => {
            serde_json::json!({"ok": false, "error": "no_duration"})
        }
        Ok(CompileOutcome::EpisodeNotFound) => {
            serde_json::json!({"ok": false, "error": format!("episode not found: {episode_id}")})
        }
        Err(e) => serde_json::json!({"ok": false, "error": e}),
    }
}

fn compile(
    store: &Arc<Mutex<PodcastStore>>,
    episode_id: &str,
) -> Result<CompileOutcome, String> {
    let snapshot = {
        let store = store.lock().map_err(|_| "store poisoned".to_owned())?;
        read_episode_inputs(&store, episode_id)
    };
    let inputs = match snapshot {
        EpisodeInputs::Missing => return Ok(CompileOutcome::EpisodeNotFound),
        EpisodeInputs::HasChapters => return Ok(CompileOutcome::AlreadyHasChapters),
        EpisodeInputs::Ready { duration_secs, transcript_present } => {
            if !transcript_present {
                return Ok(CompileOutcome::NoTranscript);
            }
            match duration_secs {
                Some(d) if d > 0.0 => d,
                _ => return Ok(CompileOutcome::NoDuration),
            }
        }
    };

    let chapters = build_stub_chapters(inputs, STUB_CHAPTER_COUNT);
    let chapter_count = chapters.len();

    let mut store = store.lock().map_err(|_| "store poisoned".to_owned())?;
    if !store.set_episode_chapters(episode_id, chapters) {
        return Ok(CompileOutcome::EpisodeNotFound);
    }
    Ok(CompileOutcome::Compiled { chapter_count })
}

enum EpisodeInputs {
    Missing,
    HasChapters,
    Ready {
        duration_secs: Option<f64>,
        transcript_present: bool,
    },
}

fn read_episode_inputs(store: &PodcastStore, episode_id: &str) -> EpisodeInputs {
    let chapters_state = store.episode_chapters_state(episode_id);
    let Some((_url, loaded)) = chapters_state else {
        return EpisodeInputs::Missing;
    };
    if loaded {
        return EpisodeInputs::HasChapters;
    }
    let duration_secs = store.episode_duration_secs(episode_id);
    let transcript_present = store
        .transcript_for(episode_id)
        .map(|t| !t.trim().is_empty())
        .unwrap_or(false);
    EpisodeInputs::Ready { duration_secs, transcript_present }
}

/// Slice the episode duration into `count` evenly-spaced AI chapters.
///
/// Always returns exactly `count` chapters; caller is responsible for
/// ensuring `count > 0` and the duration is positive. Chapter `i`'s
/// `start_secs` is `i * (duration / count)`, so chapter 0 always starts
/// at 0 and the last chapter starts at `(count-1)/count * duration`.
pub(crate) fn build_stub_chapters(duration_secs: f64, count: usize) -> Vec<Chapter> {
    let count = count.max(1);
    let step = duration_secs / count as f64;
    (0..count)
        .map(|i| {
            let mut chapter = Chapter::new(format!("Chapter {}", i + 1), i as f64 * step);
            chapter.is_ai_generated = true;
            chapter
        })
        .collect()
}

#[cfg(test)]
#[path = "ai_chapters_tests.rs"]
mod tests;
