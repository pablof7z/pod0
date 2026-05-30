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

use podcast_core::{Chapter, ChapterSource};
use tokio::runtime::Runtime;

use crate::ai_chapters_llm::{self, PromptStyle, SynthesizedChapter};
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
        let outcome = tokio::task::spawn_blocking(move || {
            synthesize_with_fallback(&episode_title, &transcript, duration_secs, &runtime_c)
        })
        .await
        // A join error (panic in the blocking worker) is itself a definitive
        // failure to reach a usable model → degrade to the stub.
        .unwrap_or_else(|_| {
            SynthOutcome::Chapters(build_stub_chapters(
                duration_secs,
                STUB_CHAPTER_COUNT,
                ChapterSource::Stub,
            ))
        });

        // Only persist + bump rev when we actually produced chapters. A
        // reachable-but-unparseable model (`GaveUp`) leaves the episode
        // chapterless so the UI can re-trigger rather than show fake slices.
        match outcome {
            SynthOutcome::Chapters(chapters) => {
                if let Ok(mut s) = store_c.lock() {
                    s.set_episode_chapters(&episode_id_c, chapters);
                }
                rev_c.fetch_add(1, Ordering::Relaxed);
            }
            SynthOutcome::GaveUp(err) => {
                eprintln!(
                    "[ai_chapters] model reachable but unparseable for {episode_id_c} \
                     after retry ({err}); leaving episode chapterless rather than \
                     emitting equal-length stubs"
                );
            }
        }
    });

    serde_json::json!({"ok": true, "status": "compiling", "episode_id": episode_id})
}

/// Maximum characters of transcript fed to the model. Wider than the original
/// 3000 so the grounding prompt sees enough of the conversation to spot real
/// topic transitions, while still bounding the request size for a small local
/// model.
const TRANSCRIPT_EXCERPT_CHARS: usize = 6000;

/// What the synthesis ladder decided to do with the episode.
#[derive(Debug)]
enum SynthOutcome {
    /// Persist these chapters (LLM-grounded or, on definitive unavailability,
    /// equal-length stubs).
    Chapters(Vec<Chapter>),
    /// The model was reachable but produced nothing parseable even after a
    /// simpler-prompt retry. Persist nothing — equal slices would misrepresent
    /// confidence. The string is the last parse error (logged by the caller).
    GaveUp(String),
}

/// Run the transcript-grounded synthesis ladder. Called from `spawn_blocking`
/// so the `runtime.block_on` inside `synthesize_chapters` is safe.
///
/// Fallback policy (the core of this milestone):
///
/// 1. Ground a first attempt in the transcript excerpt.
/// 2. On a **parse** failure (Ollama answered, response unusable) retry once
///    with a simpler JSON-only prompt — the model is present, so a stub would
///    lie about confidence.
/// 3. On an **unavailable** failure (connection refused / timeout) at any
///    point, fall back to equal-length [`ChapterSource::Stub`] chapters so the
///    feature still works offline.
/// 4. If the model stays reachable but unparseable through the retry, give up
///    rather than fabricate equal slices.
fn synthesize_with_fallback(
    episode_title: &str,
    transcript: &str,
    duration_secs: f64,
    runtime: &Arc<Runtime>,
) -> SynthOutcome {
    let excerpt: String = transcript.chars().take(TRANSCRIPT_EXCERPT_CHARS).collect();

    // Attempt 1: transcript-grounded prompt (the public default entry point).
    let first = ai_chapters_llm::synthesize_chapters(
        episode_title,
        &excerpt,
        duration_secs,
        STUB_CHAPTER_COUNT,
        runtime,
    );
    if let Some(outcome) = first_attempt_outcome(first, duration_secs, STUB_CHAPTER_COUNT) {
        return outcome;
    }

    // First attempt hit a Parse error (model reachable). Retry once with a
    // simpler JSON-only prompt, then take the terminal outcome.
    let retry = ai_chapters_llm::synthesize_chapters_styled(
        episode_title,
        &excerpt,
        duration_secs,
        STUB_CHAPTER_COUNT,
        PromptStyle::Simple,
        runtime,
    );
    terminal_outcome(retry, duration_secs, STUB_CHAPTER_COUNT)
}

/// Decide the outcome of the first (grounded) attempt. Returns `None` to mean
/// "the model is reachable but answered unparseably — retry with a simpler
/// prompt"; `Some(_)` is a terminal decision for this attempt.
///
/// Pure over the typed result so the whole fallback ladder is unit-testable
/// without a live model — the core of this milestone.
fn first_attempt_outcome(
    result: Result<Vec<SynthesizedChapter>, ai_chapters_llm::SynthError>,
    duration_secs: f64,
    count: usize,
) -> Option<SynthOutcome> {
    match result {
        Ok(synthesized) => Some(SynthOutcome::Chapters(chapters_from_llm(&synthesized))),
        // Model definitively absent → stub is the right fallback.
        Err(e) if e.is_unavailable() => Some(SynthOutcome::Chapters(build_stub_chapters(
            duration_secs,
            count,
            ChapterSource::Stub,
        ))),
        // Parse failure: model is present, signal the caller to retry.
        Err(_) => None,
    }
}

/// Decide the terminal outcome after the simpler-prompt retry. Unlike the
/// first attempt there's no further retry, so a Parse failure here is the end
/// of the line: give up rather than fabricate equal slices.
///
/// Pure over the typed result (see [`first_attempt_outcome`]).
fn terminal_outcome(
    result: Result<Vec<SynthesizedChapter>, ai_chapters_llm::SynthError>,
    duration_secs: f64,
    count: usize,
) -> SynthOutcome {
    match result {
        Ok(synthesized) => SynthOutcome::Chapters(chapters_from_llm(&synthesized)),
        // The model went away between attempts → stub.
        Err(e) if e.is_unavailable() => SynthOutcome::Chapters(build_stub_chapters(
            duration_secs,
            count,
            ChapterSource::Stub,
        )),
        // Still unparseable while reachable → don't fabricate; give up.
        Err(e) => SynthOutcome::GaveUp(e.message().to_owned()),
    }
}

/// Map LLM-synthesized chapters into `podcast_core::Chapter`s stamped with
/// [`ChapterSource::Llm`] provenance + the `is_ai_generated` flag.
fn chapters_from_llm(synthesized: &[SynthesizedChapter]) -> Vec<Chapter> {
    synthesized.iter().map(chapter_from_synthesized).collect()
}

/// Convert one [`SynthesizedChapter`] into a `podcast_core::Chapter`, stamping
/// the `is_ai_generated` flag (the constructor defaults it to `false`) and
/// [`ChapterSource::Llm`] provenance.
fn chapter_from_synthesized(c: &SynthesizedChapter) -> Chapter {
    let mut chapter = Chapter::new(c.title.clone(), c.start_secs);
    chapter.is_ai_generated = true;
    chapter.source = ChapterSource::Llm;
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

/// Slice the episode duration into `count` evenly-spaced AI chapters, stamped
/// with the given [`ChapterSource`] provenance.
///
/// Always returns exactly `count` chapters; caller is responsible for
/// ensuring `count > 0` and the duration is positive. Chapter `i`'s
/// `start_secs` is `i * (duration / count)`, so chapter 0 always starts
/// at 0 and the last chapter starts at `(count-1)/count * duration`.
///
/// Pass [`ChapterSource::Stub`] for the offline fallback path so the
/// projection can flag these as low-confidence placeholders.
pub(crate) fn build_stub_chapters(
    duration_secs: f64,
    count: usize,
    source: ChapterSource,
) -> Vec<Chapter> {
    let count = count.max(1);
    let step = duration_secs / count as f64;
    (0..count)
        .map(|i| {
            let mut chapter = Chapter::new(format!("Chapter {}", i + 1), i as f64 * step);
            chapter.is_ai_generated = true;
            chapter.source = source;
            chapter
        })
        .collect()
}

#[cfg(test)]
#[path = "ai_chapters_tests.rs"]
mod tests;
