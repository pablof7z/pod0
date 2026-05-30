//! Tests for [`super::tts`] — TtsEpisodeHandler action routing coverage.
//!
//! Extracted from `tts.rs` to keep that file under the 500-line hard limit.

use super::*;

fn empty_handler() -> (TtsEpisodeHandler, Arc<Mutex<Vec<TtsEpisodeSummary>>>) {
    let list = Arc::new(Mutex::new(Vec::new()));
    let rev = Arc::new(AtomicU64::new(0));
    // `app` pointer is only used by `handle_play` to call
    // `dispatch_capability`. The other handlers don't deref it, so
    // a null pointer is safe for these tests.
    let h = TtsEpisodeHandler::new(std::ptr::null_mut(), list.clone(), rev);
    (h, list)
}

#[test]
fn generate_with_default_length_yields_5_minute_estimate() {
    let (h, list) = empty_handler();
    let response = h.handle(
        TtsEpisodeAction::Generate {
            topic: "AI news".into(),
            length_minutes: None,
        },
        "corr-1",
        None,
    );
    assert_eq!(response["ok"], true);
    let episode_id = response["episode_id"].as_str().expect("episode_id");
    let stored = list.lock().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].id, episode_id);
    assert_eq!(stored[0].title, "AI news");
    assert_eq!(stored[0].duration_estimate_secs, 300.0);
    // With no runtime (unit-test path), the episode stays in the optimistic
    // "generating" state — the background LLM task that flips it to "ready"
    // only spawns when a runtime is wired (production path).
    assert_eq!(stored[0].status, "generating");
    assert_eq!(stored[0].script, GENERATING_PLACEHOLDER);
}

#[test]
fn generate_clamps_length_to_15_minutes() {
    let (h, list) = empty_handler();
    h.handle(
        TtsEpisodeAction::Generate {
            topic: "Too long".into(),
            length_minutes: Some(99),
        },
        "corr-1",
        None,
    );
    assert_eq!(list.lock().unwrap()[0].duration_estimate_secs, 15.0 * 60.0);
}

#[test]
fn generate_clamps_length_to_at_least_one_minute() {
    let (h, list) = empty_handler();
    h.handle(
        TtsEpisodeAction::Generate {
            topic: "Zero".into(),
            length_minutes: Some(0),
        },
        "corr-1",
        None,
    );
    assert_eq!(list.lock().unwrap()[0].duration_estimate_secs, 60.0);
}

#[test]
fn generate_rejects_empty_topic() {
    let (h, list) = empty_handler();
    let response = h.handle(
        TtsEpisodeAction::Generate {
            topic: "   ".into(),
            length_minutes: None,
        },
        "corr-1",
        None,
    );
    assert_eq!(response["ok"], false);
    assert!(list.lock().unwrap().is_empty());
}

#[test]
fn delete_removes_matching_episode() {
    let (h, list) = empty_handler();
    let response = h.handle(
        TtsEpisodeAction::Generate {
            topic: "Topic".into(),
            length_minutes: None,
        },
        "corr-1",
        None,
    );
    let id = response["episode_id"].as_str().unwrap().to_string();
    assert_eq!(list.lock().unwrap().len(), 1);
    let del = h.handle(TtsEpisodeAction::Delete { episode_id: id }, "corr-1", None);
    assert_eq!(del["ok"], true);
    assert!(list.lock().unwrap().is_empty());
}

#[test]
fn delete_unknown_id_is_idempotent_ok() {
    let (h, _list) = empty_handler();
    let del = h.handle(
        TtsEpisodeAction::Delete {
            episode_id: "nope".into(),
        },
        "corr-1",
        None,
    );
    assert_eq!(del["ok"], true);
}

#[test]
fn play_unknown_id_returns_error() {
    let (h, _list) = empty_handler();
    let response = h.handle(
        TtsEpisodeAction::Play {
            episode_id: "nope".into(),
        },
        "corr-1",
        None,
    );
    assert_eq!(response["ok"], false);
}

#[test]
fn derive_title_collapses_whitespace_and_caps_length() {
    assert_eq!(derive_title("  hello   world  "), "hello world");
    let long = "x".repeat(200);
    let title = derive_title(&long);
    // 77 chars + "…"
    assert_eq!(title.chars().count(), 78);
    assert!(title.ends_with('…'));
}

#[test]
fn placeholder_script_contains_topic() {
    let script = placeholder_script("Rustlang");
    assert!(script.contains("Rustlang"));
}
