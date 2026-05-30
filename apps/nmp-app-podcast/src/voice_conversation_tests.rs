//! Tests for the voice conversation core ([`run_turn`]).
//!
//! These target the FFI-free core directly so they need no `NmpApp`. The
//! production [`VoiceConversationManager::on_transcript_final`] wraps
//! exactly this function in `tokio::task::spawn_blocking`, so testing the
//! core synchronously honours the milestone's intent (user/assistant turn
//! accumulation) without depending on the background-task plumbing.
//!
//! No live Ollama is required: `chat_with_tools` targets
//! `localhost:11434`, which fails fast with connection-refused when no
//! model server is running, so `run_turn` deterministically takes its
//! `SCAFFOLD_ASSISTANT_REPLY` fallback path and still appends the
//! assistant turn.

use super::*;
use crate::store::PodcastStore;

fn fixtures() -> (ConversationHistory, Arc<Mutex<PodcastStore>>, Runtime) {
    let history: ConversationHistory = Arc::new(Mutex::new(Vec::new()));
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    (history, store, runtime)
}

#[test]
fn voice_finished_with_empty_transcript_is_noop() {
    let (history, store, runtime) = fixtures();

    let reply = run_turn(&history, "", &store, &runtime);
    assert!(reply.is_none(), "empty transcript must not produce a reply");
    assert!(
        history.lock().unwrap().is_empty(),
        "empty transcript must not push any turns"
    );
}

#[test]
fn voice_finished_with_whitespace_transcript_is_noop() {
    let (history, store, runtime) = fixtures();

    let reply = run_turn(&history, "   \n\t ", &store, &runtime);
    assert!(reply.is_none());
    assert!(history.lock().unwrap().is_empty());
}

#[test]
fn conversation_history_accumulates() {
    let (history, store, runtime) = fixtures();

    // Two successive final transcripts → history grows to u + a + u + a.
    let first = run_turn(&history, "what should I listen to?", &store, &runtime);
    assert!(first.is_some(), "non-empty transcript yields a speakable reply");

    let second = run_turn(&history, "tell me more", &store, &runtime);
    assert!(second.is_some());

    let h = history.lock().unwrap();
    assert_eq!(h.len(), 4, "expected user+assistant for each of two turns");
    assert_eq!(h[0].0, "user");
    assert_eq!(h[0].1, "what should I listen to?");
    assert_eq!(h[1].0, "assistant");
    assert_eq!(h[2].0, "user");
    assert_eq!(h[2].1, "tell me more");
    assert_eq!(h[3].0, "assistant");
}

#[test]
fn assistant_turn_appended_even_when_model_unreachable() {
    // With no Ollama running, the reply is the scaffold fallback — but it
    // must still be recorded as the assistant turn so the transcript stays
    // a clean alternating sequence.
    let (history, store, runtime) = fixtures();

    let reply = run_turn(&history, "hello", &store, &runtime).expect("reply");
    let h = history.lock().unwrap();
    assert_eq!(h.len(), 2);
    assert_eq!(h[1].0, "assistant");
    assert_eq!(h[1].1, reply);
}
