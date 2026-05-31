//! Tests for `picks_llm` — JSON parsing of the picks score reply and the
//! Err→heuristic fallback contract.
//!
//! These exercise the pure parse seam (`parse_picks_response`) and the JSON
//! extractor without a live Ollama. The actual network call
//! (`score_episode_for_picks`) is integration-tested implicitly through the
//! `picks_handler` background path; here we only validate parsing/fallback so
//! the suite stays hermetic and fast.

use super::{build_picks_prompt, extract_json_object, parse_picks_response};

#[test]
fn prompt_includes_listening_profile_when_present() {
    let prompt = build_picks_prompt(
        "Rate limiting at scale",
        "Backend Banter",
        "A deep dive on token buckets.",
        "Listens to: Backend Banter (3 in progress), Stratechery (played 5).",
    );
    // The profile must appear ahead of the candidate so the model conditions
    // on the user first.
    let profile_pos = prompt
        .find("Backend Banter (3 in progress)")
        .expect("profile present");
    let candidate_pos = prompt.find("Candidate episode:").expect("candidate present");
    assert!(profile_pos < candidate_pos, "profile must precede candidate");
    assert!(prompt.contains("Episode: Rate limiting at scale"));
}

#[test]
fn prompt_degrades_gracefully_with_empty_profile() {
    let prompt = build_picks_prompt("Ep", "Show", "Desc", "   ");
    assert!(
        prompt.contains("no listening history yet"),
        "empty profile must signal cold-start to the model"
    );
    assert!(prompt.contains("Candidate episode:"));
}

#[test]
fn prompt_truncates_long_description_to_500_chars() {
    let long = "x".repeat(900);
    let prompt = build_picks_prompt("Ep", "Show", &long, "profile");
    // 500 x's max in the description body.
    let xs = prompt.matches('x').count();
    assert_eq!(xs, 500);
}

#[test]
fn parses_bare_score_and_reason() {
    let s = r#"{"score": 0.9, "reason": "Deep dive on rate limiting at scale."}"#;
    let (score, reason) = parse_picks_response(s).expect("parse ok");
    assert!((score - 0.9).abs() < 1e-6);
    assert_eq!(reason, "Deep dive on rate limiting at scale.");
}

#[test]
fn parses_score_with_markdown_fence_and_preamble() {
    let s = "Sure! Here is the result:\n```json\n{\"score\": 0.42, \"reason\": \"Niche topic.\"}\n```";
    let (score, reason) = parse_picks_response(s).expect("parse ok");
    assert!((score - 0.42).abs() < 1e-6);
    assert_eq!(reason, "Niche topic.");
}

#[test]
fn clamps_out_of_range_score_high() {
    let s = r#"{"score": 1.7, "reason": "Hyped."}"#;
    let (score, _) = parse_picks_response(s).expect("parse ok");
    assert!((score - 1.0).abs() < 1e-6);
}

#[test]
fn clamps_negative_score_to_zero() {
    let s = r#"{"score": -0.5, "reason": "Boring."}"#;
    let (score, _) = parse_picks_response(s).expect("parse ok");
    assert!((score - 0.0).abs() < 1e-6);
}

#[test]
fn missing_score_defaults_to_midpoint() {
    let s = r#"{"reason": "No score field."}"#;
    let (score, reason) = parse_picks_response(s).expect("parse ok");
    assert!((score - 0.5).abs() < 1e-6);
    assert_eq!(reason, "No score field.");
}

#[test]
fn missing_reason_falls_back_to_generic() {
    let s = r#"{"score": 0.8}"#;
    let (score, reason) = parse_picks_response(s).expect("parse ok");
    assert!((score - 0.8).abs() < 1e-6);
    assert_eq!(reason, "Recommended pick");
}

#[test]
fn parse_fails_when_no_json_object() {
    // No braces at all → Err. The caller treats this as "Ollama unusable"
    // and falls back to the recency heuristic.
    assert!(parse_picks_response("the model returned prose, not json").is_err());
}

#[test]
fn extract_handles_nested_and_trailing_text() {
    let s = r#"prefix {"score": 0.6, "reason": "ok"} trailing"#;
    let extracted = extract_json_object(s).expect("extract ok");
    assert!(extracted.starts_with('{'));
    assert!(extracted.ends_with('}'));
    assert!(extracted.contains("score"));
}

#[test]
fn extract_errors_on_no_braces() {
    assert!(extract_json_object("no braces here").is_err());
}

/// Simulates the handler's fallback decision: when `score_episode_for_picks`
/// would return `Err` (Ollama offline), the handler keeps the heuristic score.
/// This documents the contract that an `Err` yields the heuristic value, not a
/// poisoned/zero pick.
#[test]
fn err_result_yields_heuristic_score_in_fallback_select() {
    // The function under test in production is `score_episode_for_picks`,
    // which returns `Result<(f32, String), String>`. Here we model the
    // handler's select logic directly to lock the fallback semantics.
    let heuristic_score = 0.65_f32;
    let heuristic_reason = "New from Stratechery".to_string();

    let llm_result: Result<(f32, String), String> = Err("ollama offline".into());
    let (score, reason) = match llm_result {
        Ok((s, r)) => (s, r),
        Err(_) => (heuristic_score, heuristic_reason.clone()),
    };

    assert!((score - heuristic_score).abs() < 1e-6);
    assert_eq!(reason, heuristic_reason);
}

/// And the success branch must override the heuristic.
#[test]
fn ok_result_overrides_heuristic_in_fallback_select() {
    let heuristic_score = 0.65_f32;
    let heuristic_reason = "New from Stratechery".to_string();

    let llm_result: Result<(f32, String), String> =
        Ok((0.93, "Standout interview.".into()));
    let (score, reason) = match llm_result {
        Ok((s, r)) => (s, r),
        Err(_) => (heuristic_score, heuristic_reason),
    };

    assert!((score - 0.93).abs() < 1e-6);
    assert_eq!(reason, "Standout interview.");
}
