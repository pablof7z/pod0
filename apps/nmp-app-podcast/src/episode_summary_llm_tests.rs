use super::*;

#[test]
fn build_prompt_prefers_transcript_when_present() {
    let prompt = build_prompt("My Title", "the description", Some("the transcript"));
    assert!(prompt.contains("Episode title: My Title"));
    assert!(prompt.contains("the transcript"));
    assert!(!prompt.contains("the description"));
}

#[test]
fn build_prompt_falls_back_to_description_without_transcript() {
    let prompt = build_prompt("My Title", "the description", None);
    assert!(prompt.contains("the description"));
}

#[test]
fn build_prompt_falls_back_when_transcript_blank() {
    let prompt = build_prompt("T", "desc body", Some("   \n  "));
    assert!(prompt.contains("desc body"));
}

#[test]
fn build_prompt_truncates_oversized_body() {
    let big = "x".repeat(MAX_BODY_CHARS + 5_000);
    let prompt = build_prompt("T", &big, None);
    let body_len = prompt.matches('x').count();
    assert_eq!(body_len, MAX_BODY_CHARS);
}

#[test]
fn clean_summary_trims_whitespace() {
    assert_eq!(
        clean_summary("  A tidy summary.\n").unwrap(),
        "A tidy summary."
    );
}

#[test]
fn clean_summary_rejects_empty() {
    assert!(clean_summary("   \n  ").is_err());
    assert!(clean_summary("").is_err());
}
