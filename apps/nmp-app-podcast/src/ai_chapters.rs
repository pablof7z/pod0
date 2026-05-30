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
use tokio::runtime::Runtime;

use crate::ai_chapters_llm::{self, SynthesizedChapter};
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
    rev: &Arc<AtomicU64>,
    runtime: &Arc<Runtime>,
    episode_id: String,
) -> serde_json::Value {
    // Gate checks run synchronously (fast, no I/O) so errors surface immediately.
    let snapshot = match store.lock() {
        Ok(s) => read_episode_inputs(&s, &episode_id),
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };
    let (duration_secs, transcript, episode_title) = match snapshot {
        EpisodeInputs::Missing => {
            return serde_json::json!({"ok": false, "error": format!("episode not found: {episode_id}")})
        }
        EpisodeInputs::HasChapters => {
            return serde_json::json!({"ok": true, "status": "already_has_chapters"})
        }
        EpisodeInputs::Ready { duration_secs, transcript, episode_title } => {
            let Some(transcript) = transcript else {
                return serde_json::json!({"ok": false, "error": "no_transcript"});
            };
            let duration_secs = match duration_secs {
                Some(d) if d > 0.0 => d,
                _ => return serde_json::json!({"ok": false, "error": "no_duration"}),
            };
            (duration_secs, transcript, episode_title)
        }
    };

    // M5.5 fix: spawn LLM synthesis off the actor thread (same pattern as M5.1
    // inbox triage). The actor returns immediately; chapters land in the store
    // when the background task completes and bump rev for the next snapshot.
    let store_c = Arc::clone(store);
    let rev_c = Arc::clone(rev);
    let runtime_c = Arc::clone(runtime);
    let episode_id_c = episode_id.clone();

    runtime.spawn(async move {
        let chapters = tokio::task::spawn_blocking(move || {
            synthesize_or_stub(&episode_title, &transcript, duration_secs, &runtime_c)
        })
        .await
        .unwrap_or_else(|_| build_stub_chapters(duration_secs, STUB_CHAPTER_COUNT));

        if let Ok(mut s) = store_c.lock() {
            s.set_episode_chapters(&episode_id_c, chapters);
        }
        rev_c.fetch_add(1, Ordering::Relaxed);
    });

    serde_json::json!({"ok": true, "status": "compiling", "episode_id": episode_id})
}

/// Synthesize chapters via the LLM, degrading to the equal-length stub on any
/// error so the feature stays usable offline. Called from `spawn_blocking` so
/// the `runtime.block_on` inside `synthesize_chapters` is safe.
fn synthesize_or_stub(
    episode_title: &str,
    transcript: &str,
    duration_secs: f64,
    runtime: &Arc<Runtime>,
) -> Vec<Chapter> {
    let transcript_excerpt: String = transcript.chars().take(3000).collect();
    match ai_chapters_llm::synthesize_chapters(
        episode_title,
        &transcript_excerpt,
        duration_secs,
        STUB_CHAPTER_COUNT,
        runtime,
    ) {
        Ok(synthesized) if !synthesized.is_empty() => {
            synthesized.iter().map(chapter_from_synthesized).collect()
        }
        _ => build_stub_chapters(duration_secs, STUB_CHAPTER_COUNT),
    }
}

/// Convert one [`SynthesizedChapter`] into a `podcast_core::Chapter`, stamping
/// the `is_ai_generated` flag (the constructor defaults it to `false`).
fn chapter_from_synthesized(c: &SynthesizedChapter) -> Chapter {
    let mut chapter = Chapter::new(c.title.clone(), c.start_secs);
    chapter.is_ai_generated = true;
    chapter
}

enum EpisodeInputs {
    Missing,
    HasChapters,
    Ready {
        duration_secs: Option<f64>,
        /// The cached transcript text, or `None` when absent / whitespace-only.
        transcript: Option<String>,
        episode_title: String,
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
    let transcript = store
        .transcript_for(episode_id)
        .filter(|t| !t.trim().is_empty())
        .map(str::to_owned);
    let episode_title = store
        .episode_titles_and_duration(episode_id)
        .map(|(ep_title, _pod_title, _dur)| ep_title)
        .unwrap_or_default();
    EpisodeInputs::Ready { duration_secs, transcript, episode_title }
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
