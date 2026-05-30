use super::*;

use crate::store::PodcastStore;

#[test]
fn system_prompt_includes_memory_facts_when_store_has_facts() {
    let mut store = PodcastStore::new();
    store.set_memory_fact("preferred_genre".into(), "true crime".into(), "user".into(), 1);
    let store = Arc::new(Mutex::new(store));

    let prompt = build_system_prompt_with_memory(Some(&store));

    assert!(
        prompt.contains("preferred_genre"),
        "prompt must include the fact key, got: {prompt}"
    );
    assert!(
        prompt.contains("true crime"),
        "prompt must include the fact value, got: {prompt}"
    );
    assert!(
        prompt.contains("User memory facts"),
        "prompt must label the memory section, got: {prompt}"
    );
    // The base prompt is still present alongside the facts.
    assert!(prompt.contains(AGENT_SYSTEM_PROMPT));
}

#[test]
fn system_prompt_is_base_when_no_facts() {
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    assert_eq!(build_system_prompt_with_memory(Some(&store)), AGENT_SYSTEM_PROMPT);
    // No store at all (scaffold path) also yields just the base prompt.
    assert_eq!(build_system_prompt_with_memory(None), AGENT_SYSTEM_PROMPT);
}

fn fresh_handler() -> AgentChatHandler {
    AgentChatHandler::new_without_runtime(
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicU64::new(0)),
    )
}

#[test]
fn send_appends_user_and_assistant() {
    let h = fresh_handler();
    let res = h.handle(AgentChatAction::Send {
        message: "Hello there".into(),
    });
    assert_eq!(res["ok"], true);
    let c = h.conversation.lock().unwrap();
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].role, "user");
    assert_eq!(c[0].content, "Hello there");
    assert!(!c[0].is_generating);
    assert_eq!(c[1].role, "assistant");
    // Without a runtime the handler falls back to the scaffold reply.
    assert!(!c[1].content.is_empty(), "assistant content must not be empty");
    assert!(!c[1].is_generating);
    assert!(h.touched.load(Ordering::Relaxed));
    assert!(!h.busy.load(Ordering::Relaxed));
    assert!(h.rev.load(Ordering::Relaxed) >= 1);
}

/// Explicit test for the fallback path: when no runtime is wired in,
/// the handler must return the scaffold constant rather than panicking or
/// returning an empty string.
#[test]
fn scaffold_reply_is_fallback_when_llm_unavailable() {
    let h = fresh_handler();
    let _ = h.handle(AgentChatAction::Send {
        message: "What is RSS?".into(),
    });
    let c = h.conversation.lock().unwrap();
    assert_eq!(c[1].role, "assistant");
    assert_eq!(
        c[1].content, SCAFFOLD_ASSISTANT_REPLY,
        "fallback must be the scaffold constant when no runtime is available"
    );
    assert!(!c[1].is_generating);
}

#[test]
fn send_trims_input_and_rejects_empty() {
    let h = fresh_handler();
    let res = h.handle(AgentChatAction::Send {
        message: "   ".into(),
    });
    assert_eq!(res["ok"], false);
    assert_eq!(res["error"], "empty message");
    assert!(h.conversation.lock().unwrap().is_empty());
    let res = h.handle(AgentChatAction::Send {
        message: "  what's new?  ".into(),
    });
    assert_eq!(res["ok"], true);
    let c = h.conversation.lock().unwrap();
    assert_eq!(c[0].content, "what's new?");
}

#[test]
fn clear_wipes_transcript_but_keeps_touched() {
    let h = fresh_handler();
    let _ = h.handle(AgentChatAction::Send {
        message: "hi".into(),
    });
    assert_eq!(h.conversation.lock().unwrap().len(), 2);
    let res = h.handle(AgentChatAction::Clear);
    assert_eq!(res["ok"], true);
    assert!(h.conversation.lock().unwrap().is_empty());
    // Touched stays true so the projection emits an empty `Some(agent)`
    // rather than reverting to `None`.
    assert!(h.touched.load(Ordering::Relaxed));
}

#[test]
fn message_ids_are_unique() {
    let h = fresh_handler();
    for _ in 0..3 {
        let _ = h.handle(AgentChatAction::Send {
            message: "ping".into(),
        });
    }
    let c = h.conversation.lock().unwrap();
    let mut ids: Vec<&str> = c.iter().map(|m| m.id.as_str()).collect();
    let total = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), total, "every message must have a unique id");
}

#[test]
fn rev_bumps_on_each_mutation() {
    let h = fresh_handler();
    let start = h.rev.load(Ordering::Relaxed);
    let _ = h.handle(AgentChatAction::Send {
        message: "first".into(),
    });
    let after_send = h.rev.load(Ordering::Relaxed);
    let _ = h.handle(AgentChatAction::Clear);
    let after_clear = h.rev.load(Ordering::Relaxed);
    assert!(after_send > start);
    assert!(after_clear > after_send);
}
