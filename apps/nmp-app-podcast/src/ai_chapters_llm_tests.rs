//! Tests for [`super::ai_chapters_llm`] — JSON parsing + array extraction.
//!
//! The live `synthesize_chapters` round-trip is not exercised here (it needs a
//! running Ollama instance); we test the deterministic parse/extract seam that
//! the round-trip funnels its response through.

use super::{
    extract_json_array, parse_chapters, synthesize_chapters, SynthError, SynthesizedChapter,
};

#[test]
fn parse_valid_json_chapters() {
    let json = r#"[{"title":"Intro and welcome","start_secs":0.0},{"title":"Deep dive on Rust","start_secs":120.5}]"#;
    let chapters = parse_chapters(json).expect("valid JSON should parse");
    assert_eq!(
        chapters,
        vec![
            SynthesizedChapter { title: "Intro and welcome".into(), start_secs: 0.0 },
            SynthesizedChapter { title: "Deep dive on Rust".into(), start_secs: 120.5 },
        ]
    );
}

#[test]
fn parse_with_preamble_ignores_extra_text() {
    let response = r#"Sure! Here are your chapters:
[{"title":"Opening remarks","start_secs":0.0},{"title":"Guest interview","start_secs":300.0}]
Hope that helps!"#;
    // The array extractor must pull just the [ ... ] slice out of the prose.
    let extracted = extract_json_array(response).expect("array present");
    assert!(extracted.starts_with('['));
    assert!(extracted.ends_with(']'));

    let chapters = parse_chapters(response).expect("array embedded in prose should parse");
    assert_eq!(chapters.len(), 2);
    assert_eq!(chapters[0].title, "Opening remarks");
    assert_eq!(chapters[1].start_secs, 300.0);
}

#[test]
fn parse_rejects_unusable_responses() {
    // A reachable-but-unparseable model produces these. The caller treats them
    // as `SynthError::Parse` (retry with a simpler prompt, then give up) —
    // NOT as a reason to fabricate equal-length stub chapters.
    // Garbage with no array → Err.
    assert!(parse_chapters("I cannot help with that.").is_err());
    // An array of the wrong shape → Err.
    assert!(parse_chapters(r#"[{"name":"oops"}]"#).is_err());
    // A valid-but-empty array → Err.
    assert!(parse_chapters("[]").is_err());
    // Malformed JSON inside the brackets → Err.
    assert!(parse_chapters(r#"[{"title":"x","start_secs":}]"#).is_err());
}

#[test]
fn synth_error_discriminates_unavailable_from_parse() {
    // The fallback ladder hinges on this split: only `Unavailable` justifies
    // the equal-length stub; `Parse` means the model is present and answered.
    assert!(SynthError::Unavailable("connection refused".into()).is_unavailable());
    assert!(!SynthError::Parse("no JSON array".into()).is_unavailable());
    assert_eq!(SynthError::Parse("boom".into()).message(), "boom");
}

/// Offline `synthesize_chapters` against a port with nothing listening must be
/// classified as `Unavailable` (not `Parse`) so the caller stubs rather than
/// retries forever. Uses the real (no-Ollama) localhost path; fast because the
/// connection is refused immediately.
#[test]
fn synthesize_offline_is_unavailable() {
    let rt = std::sync::Arc::new(tokio::runtime::Runtime::new().unwrap());
    let result = synthesize_chapters("Ep", "some transcript text", 600.0, 5, &rt);
    match result {
        Err(e) => assert!(
            e.is_unavailable(),
            "offline Ollama must classify as Unavailable, got: {e:?}"
        ),
        // If a dev machine happens to have Ollama up, a successful parse is fine
        // too — we only assert the *error* shape, never that it must fail.
        Ok(_) => {}
    }
}

/// Live round-trip against a running Ollama instance (`deepseek-v4-flash:cloud`
/// at localhost:11434). Ignored by default so the suite stays offline-clean;
/// run with `cargo test -p nmp-app-podcast -- --ignored --nocapture` to confirm
/// the real path produces prose titles (not "Chapter N") with monotonic offsets.
#[test]
#[ignore = "requires a live Ollama instance"]
fn synthesize_against_live_ollama_returns_prose_titles() {
    let rt = std::sync::Arc::new(tokio::runtime::Runtime::new().unwrap());
    let transcript = "Welcome back to the show. Today we sit down with a longtime \
        systems engineer to talk about the hard-won lessons of scaling \
        distributed databases. We start with the early architecture choices \
        that seemed clever but aged poorly, then move into how the team \
        rebuilt the storage layer twice, and finally we cover what they would \
        tell their younger selves about premature optimization and technical \
        debt. It is a candid, wide-ranging conversation about building systems \
        that have to survive contact with real production traffic.";

    let result = synthesize_chapters(
        "Lessons from Scaling Databases",
        transcript,
        3600.0,
        5,
        &rt,
    );

    let chapters = result.expect("live Ollama should return chapters");
    eprintln!("LIVE CHAPTERS: {chapters:#?}");
    assert!(!chapters.is_empty(), "expected at least one chapter");
    assert_eq!(chapters[0].start_secs, 0.0, "first chapter starts at 0.0");
    for w in chapters.windows(2) {
        assert!(
            w[1].start_secs > w[0].start_secs,
            "start_secs must increase monotonically: {:?}",
            chapters
        );
    }
    for c in &chapters {
        assert!(
            !c.title.starts_with("Chapter "),
            "title should be prose, not a stub label: {:?}",
            c.title
        );
    }
}
