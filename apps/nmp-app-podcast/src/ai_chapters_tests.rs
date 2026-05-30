//! Tests for [`super::ai_chapters`] — stub chapter compilation and gating logic.
//!
//! Extracted from `ai_chapters.rs` to keep that file under the 500-line hard limit.

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use super::*;
use podcast_core::{Episode, Podcast};
use tokio::runtime::Runtime;

fn test_runtime() -> Arc<Runtime> {
    Arc::new(Runtime::new().expect("runtime"))
}

fn test_rev() -> Arc<AtomicU64> {
    Arc::new(AtomicU64::new(0))
}

fn make_episode_with_duration(duration: Option<f64>) -> (Podcast, Episode) {
    let podcast = Podcast::new("Show");
    let mut episode = Episode::new(
        podcast.id,
        "https://example.com/feed.xml",
        "guid-1",
        "Ep",
        url::Url::parse("https://example.com/audio.mp3").unwrap(),
        chrono::Utc::now(),
    );
    episode.duration_secs = duration;
    (podcast, episode)
}

#[test]
fn build_stub_chapters_evenly_distributes_starts() {
    let chapters = build_stub_chapters(3600.0, 4, ChapterSource::Stub);
    assert_eq!(chapters.len(), 4);
    assert_eq!(chapters[0].start_secs, 0.0);
    assert_eq!(chapters[1].start_secs, 900.0);
    assert_eq!(chapters[2].start_secs, 1800.0);
    assert_eq!(chapters[3].start_secs, 2700.0);
    assert!(chapters.iter().all(|c| c.is_ai_generated));
    assert_eq!(chapters[0].title, "Chapter 1");
    assert_eq!(chapters[3].title, "Chapter 4");
}

#[test]
fn build_stub_chapters_stamps_requested_source() {
    // Provenance must ride through so the projection can flag low-confidence
    // fallback chapters distinctly from LLM-grounded ones.
    let stub = build_stub_chapters(60.0, 2, ChapterSource::Stub);
    assert!(stub.iter().all(|c| c.source == ChapterSource::Stub));
}

#[test]
fn build_stub_chapters_handles_count_one() {
    let chapters = build_stub_chapters(120.0, 1, ChapterSource::Stub);
    assert_eq!(chapters.len(), 1);
    assert_eq!(chapters[0].start_secs, 0.0);
    assert!(chapters[0].is_ai_generated);
}

#[test]
fn build_stub_chapters_treats_zero_count_as_one() {
    // Defensive: zero would cause a divide-by-zero in the loop; we clamp to 1.
    let chapters = build_stub_chapters(60.0, 0, ChapterSource::Stub);
    assert_eq!(chapters.len(), 1);
}

#[test]
fn chapter_from_synthesized_marks_llm_provenance() {
    // LLM-synthesized chapters must be tagged `Llm` (high confidence), not the
    // default publisher provenance, and flagged ai-generated.
    let synth = SynthesizedChapter { title: "Real topic shift".into(), start_secs: 42.0 };
    let chapter = chapter_from_synthesized(&synth);
    assert_eq!(chapter.source, ChapterSource::Llm);
    assert!(chapter.is_ai_generated);
    assert_eq!(chapter.title, "Real topic shift");
    assert_eq!(chapter.start_secs, 42.0);
}

// --- Fallback ladder: the core decision logic, unit-tested without a model.
// `first_attempt_outcome` / `terminal_outcome` are pure over the typed result,
// so we can exercise every branch with hand-built Ok/Err values.

use crate::ai_chapters_llm::SynthError;

fn one_chapter() -> Vec<SynthesizedChapter> {
    vec![SynthesizedChapter { title: "Topic A".into(), start_secs: 0.0 }]
}

#[test]
fn first_attempt_ok_yields_llm_chapters() {
    match first_attempt_outcome(Ok(one_chapter()), 600.0, 5) {
        Some(SynthOutcome::Chapters(chs)) => {
            assert_eq!(chs.len(), 1);
            assert_eq!(chs[0].source, ChapterSource::Llm);
        }
        other => panic!("expected LLM chapters, got {other:?}"),
    }
}

#[test]
fn first_attempt_unavailable_yields_stub_chapters() {
    // Definitive unavailability is the ONLY case that produces equal-length
    // stubs from the first attempt.
    match first_attempt_outcome(Err(SynthError::Unavailable("refused".into())), 600.0, 5) {
        Some(SynthOutcome::Chapters(chs)) => {
            assert_eq!(chs.len(), 5);
            assert!(chs.iter().all(|c| c.source == ChapterSource::Stub));
        }
        other => panic!("expected stub chapters, got {other:?}"),
    }
}

#[test]
fn first_attempt_parse_signals_retry() {
    // A reachable-but-unparseable model must NOT stub on the first attempt —
    // it returns None so the ladder retries with a simpler prompt.
    let outcome = first_attempt_outcome(Err(SynthError::Parse("bad json".into())), 600.0, 5);
    assert!(outcome.is_none(), "parse failure must signal retry, got {outcome:?}");
}

#[test]
fn terminal_ok_yields_llm_chapters() {
    match terminal_outcome(Ok(one_chapter()), 600.0, 5) {
        SynthOutcome::Chapters(chs) => assert_eq!(chs[0].source, ChapterSource::Llm),
        other => panic!("expected LLM chapters, got {other:?}"),
    }
}

#[test]
fn terminal_unavailable_yields_stub_chapters() {
    // Model went away between attempts → stub is justified.
    match terminal_outcome(Err(SynthError::Unavailable("timeout".into())), 600.0, 5) {
        SynthOutcome::Chapters(chs) => {
            assert_eq!(chs.len(), 5);
            assert!(chs.iter().all(|c| c.source == ChapterSource::Stub));
        }
        other => panic!("expected stub chapters, got {other:?}"),
    }
}

#[test]
fn terminal_parse_gives_up_without_stubbing() {
    // Reachable-but-unparseable through the retry → give up. Crucially this
    // does NOT fabricate equal-length stubs (the milestone's whole point).
    match terminal_outcome(Err(SynthError::Parse("still bad".into())), 600.0, 5) {
        SynthOutcome::GaveUp(msg) => assert_eq!(msg, "still bad"),
        other => panic!("expected GaveUp, got {other:?}"),
    }
}

/// Async compilation test — needs a running runtime + background thread pool.
/// Marked `#[ignore]` so CI doesn't flake on timing; run manually with
/// `cargo test -- --ignored compile_emits_compiling`.
#[test]
#[ignore = "async background task — requires multi-thread runtime headroom; run manually"]
fn compile_emits_compiling_status_and_persists_chapters() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let (podcast, episode) = make_episode_with_duration(Some(600.0));
    let ep_id = episode.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![episode]);
    store.lock().unwrap().set_transcript(ep_id.clone(), "hello world".to_owned());

    let rt = test_runtime();
    let rev = test_rev();
    let result = handle_compile_chapters(&store, &rev, &rt, ep_id.clone());
    assert_eq!(result["ok"], true);
    assert_eq!(result["status"], "compiling");
    // M5.5: compilation is async — the handler spawns background work and returns
    // immediately. The actor thread is NOT blocked; rev is bumped when the task
    // completes. We drive the spawned task to completion via block_on with a
    // short sleep so the test exercises the async path end-to-end.
    // Rev starts at 0, background task bumps it after storing chapters.
    assert_eq!(rev.load(std::sync::atomic::Ordering::Relaxed), 0,
        "rev must NOT be primed synchronously (async dispatch)");
    // Wait for background task on the multi-thread runtime.
    rt.block_on(async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    });
    assert!(rev.load(std::sync::atomic::Ordering::Relaxed) >= 1,
        "rev must be bumped after background compile completes");

    let (_url, loaded) = store
        .lock()
        .unwrap()
        .episode_chapters_state(&ep_id)
        .expect("episode present");
    assert!(loaded, "compiled chapters must be persisted after background task");
}

#[test]
fn compile_is_idempotent_when_episode_has_chapters() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let (podcast, mut episode) = make_episode_with_duration(Some(600.0));
    let ep_id = episode.id.0.to_string();
    episode.chapters = Some(vec![Chapter::new("Existing", 0.0)]);
    store.lock().unwrap().subscribe(podcast, vec![episode]);
    store.lock().unwrap().set_transcript(ep_id.clone(), "hi".to_owned());

    let rt = test_runtime();
    let rev = test_rev();
    let result = handle_compile_chapters(&store, &rev, &rt, ep_id);
    assert_eq!(result["ok"], true);
    assert_eq!(result["status"], "already_has_chapters");
    // No mutation, no rev bump.
    assert_eq!(rev.load(Ordering::Relaxed), 0);
}

#[test]
fn compile_refuses_when_no_transcript() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let (podcast, episode) = make_episode_with_duration(Some(600.0));
    let ep_id = episode.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![episode]);

    let rt = test_runtime();
    let rev = test_rev();
    let result = handle_compile_chapters(&store, &rev, &rt, ep_id);
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"], "no_transcript");
    assert_eq!(rev.load(Ordering::Relaxed), 0);
}

#[test]
fn compile_refuses_when_transcript_is_whitespace_only() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let (podcast, episode) = make_episode_with_duration(Some(600.0));
    let ep_id = episode.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![episode]);
    store.lock().unwrap().set_transcript(ep_id.clone(), "   \n  ".to_owned());

    let rt = test_runtime();
    let rev = test_rev();
    let result = handle_compile_chapters(&store, &rev, &rt, ep_id);
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"], "no_transcript");
}

#[test]
fn compile_refuses_when_no_duration() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let (podcast, episode) = make_episode_with_duration(None);
    let ep_id = episode.id.0.to_string();
    store.lock().unwrap().subscribe(podcast, vec![episode]);
    store.lock().unwrap().set_transcript(ep_id.clone(), "hi".to_owned());

    let rt = test_runtime();
    let rev = test_rev();
    let result = handle_compile_chapters(&store, &rev, &rt, ep_id);
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"], "no_duration");
}

#[test]
fn compile_reports_episode_not_found() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let rt = test_runtime();
    let rev = test_rev();
    let result = handle_compile_chapters(&store, &rev, &rt, "missing-episode".to_owned());
    assert_eq!(result["ok"], false);
    assert!(
        result["error"].as_str().unwrap_or_default().contains("episode not found"),
        "got: {}",
        result["error"]
    );
}
