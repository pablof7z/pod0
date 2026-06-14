//! Tests for [super::categorization_module] — action wire format, dispatch, and heuristic categorizer.
//!
//! Extracted from `categorization_module.rs` to keep that file under the 500-line hard limit.

use super::*;


/// Test helper: extract `(action_json, correlation_id)` from an
/// `ActorCommand::Protocol(HostOpCommand { .. })` via its `Debug` output.
/// HostOpCommand fields are private in nmp-core; this avoids direct access.
#[cfg(test)]
#[allow(dead_code)]
fn extract_host_op_parts(cmd: &ActorCommand) -> (String, String) {
    let dbg = format!("{cmd:?}");
    // Debug fmt: Protocol(HostOpCommand { action_json: "{..}", correlation_id: "corr" })
    // The outer string delimiters are literal " in the Debug output; inner " are \".
    let jm = concat!("action_json: ", r#"""#);
    let js = dbg.find(jm).expect("action_json") + jm.len();
    let after = &dbg[js..];
    let je = after.find(concat!(r#"""#, ", correlation_id:")).expect("json end");
    let raw = &after[..je];
    // Unescape \" → " and \\\\ → \\
    let tmp = raw.replace(r#"\\"#, "\x01BSLASH\x01");
    let action_json = tmp.replace(r#"\""#, r#"""#).replace("\x01BSLASH\x01", "\\");
    let cm = concat!("correlation_id: ", r#"""#);
    let cs = dbg.find(cm).expect("corr_id") + cm.len();
    let after_c = &dbg[cs..];
    let ce = after_c.find(concat!(r#"""#, " }")).expect("corr end");
    (action_json, after_c[..ce].to_string())
}

#[test]
fn action_ids_match_documented_strings() {
    assert_eq!(ACTION_CATEGORIZE_RUN, "podcast.categorize.run");
    assert_eq!(
        ACTION_CATEGORIZE_EPISODE,
        "podcast.categorize.categorize_episode"
    );
}

#[test]
fn run_action_round_trips() {
    let action = CategorizationAction::Run;
    let json = serde_json::to_string(&action).expect("encode");
    assert_eq!(json, r#"{"op":"run"}"#);
    let decoded: CategorizationAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn categorize_episode_action_round_trips() {
    let action = CategorizationAction::CategorizeEpisode {
        episode_id: "ep-7".into(),
    };
    let json = serde_json::to_string(&action).expect("encode");
    assert!(json.contains(r#""op":"categorize_episode""#));
    assert!(json.contains(r#""episode_id":"ep-7""#));
    let decoded: CategorizationAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, action);
}

#[test]
fn execute_emits_dispatch_host_op() {
    let action = CategorizationAction::Run;
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    CategorizationModule.execute(action, "corr-1", &|cmd| {
        commands.lock().unwrap().push(cmd);
    })
    .expect("execute ok");
    let commands = commands.into_inner().unwrap();
    assert_eq!(commands.len(), 1);
    let ActorCommand::Protocol(_) = &commands[0]
    else { panic!("expected Protocol command"); };
    let (action_json, correlation_id) = extract_host_op_parts(&commands[0]);
    assert_eq!(correlation_id.as_str(), "corr-1");
    let v: serde_json::Value = serde_json::from_str(&action_json).expect("json");
    assert_eq!(v["ns"], "podcast.categorize");
    assert_eq!(v["action"]["op"], "run");
}

// ── Heuristic categorizer ─────────────────────────────────────────

#[test]
fn categorize_text_returns_empty_for_empty_input() {
    assert!(categorize_text("", "").is_empty());
    assert!(categorize_text("   ", "  ").is_empty());
}

#[test]
fn categorize_text_picks_technology_for_software_keywords() {
    let cats = categorize_text(
        "The future of software engineering",
        "We dig into AI, machine learning, and open source tools developers love.",
    );
    assert!(cats.contains(&"Technology".to_owned()));
}

#[test]
fn categorize_text_picks_science_for_research_keywords() {
    let cats = categorize_text(
        "Quantum biology",
        "A scientist explains how quantum physics meets biology in modern research.",
    );
    assert!(cats.contains(&"Science".to_owned()));
}

#[test]
fn categorize_text_picks_business_for_finance_keywords() {
    let cats = categorize_text(
        "Startup finance 101",
        "Investors, founders, and ceos talk vc economy and ipo strategy.",
    );
    assert!(cats.contains(&"Business".to_owned()));
}

#[test]
fn categorize_text_caps_at_three_categories() {
    // A pathological episode hitting every category. Still capped.
    let cats = categorize_text(
        "Tech politics health sports culture business science education entertainment",
        "Code election medicine football art finance research school comedy.",
    );
    assert!(
        cats.len() <= MAX_CATEGORIES_PER_EPISODE,
        "got {} categories: {:?}",
        cats.len(),
        cats
    );
}

#[test]
fn categorize_text_orders_by_match_count() {
    // Two science hits, one technology hit ⇒ Science first.
    let cats = categorize_text(
        "Quantum physics research",
        "An exploration of biology and ai in the lab.",
    );
    // Both should be present.
    assert!(cats.contains(&"Science".to_owned()));
    assert!(cats.contains(&"Technology".to_owned()));
    // Science (3 hits: quantum, physics, biology, research) should
    // outrank Technology (1 hit: ai).
    let sci_idx = cats.iter().position(|c| c == "Science").unwrap();
    let tech_idx = cats.iter().position(|c| c == "Technology").unwrap();
    assert!(
        sci_idx < tech_idx,
        "expected Science before Technology in {:?}",
        cats
    );
}

#[test]
fn categorize_text_returns_empty_for_unmatched_input() {
    let cats = categorize_text("Lorem ipsum", "dolor sit amet consectetur");
    assert!(cats.is_empty(), "got {:?}", cats);
}

#[test]
fn contains_word_bounded_respects_word_boundaries() {
    // "ai" must not match inside "main" or "said".
    assert!(!contains_word_bounded("the main idea", "ai"));
    assert!(!contains_word_bounded("she said hi", "ai"));
    assert!(contains_word_bounded("we use ai today", "ai"));
    assert!(contains_word_bounded("ai is everywhere", "ai"));
    assert!(contains_word_bounded("everywhere is ai", "ai"));
}

#[test]
fn contains_word_bounded_handles_multiword_needles() {
    assert!(contains_word_bounded(
        "the open source community thrives",
        "open source"
    ));
    assert!(!contains_word_bounded(
        "wide-open sourcing today",
        "open source"
    ));
}
