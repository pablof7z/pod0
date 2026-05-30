use super::{fallback_segments, parse_briefing_array};

#[test]
fn parse_bare_array() {
    let s = r#"["First story about tech.", "Second story about science."]"#;
    let segments = parse_briefing_array(s).unwrap();
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0], "First story about tech.");
    assert_eq!(segments[1], "Second story about science.");
}

#[test]
fn parse_array_with_preamble_and_fences() {
    let s = "Sure! Here is your briefing:\n```json\n[\"Alpha segment.\", \"Beta segment.\"]\n```\nHope that helps!";
    let segments = parse_briefing_array(s).unwrap();
    assert_eq!(segments, vec!["Alpha segment.".to_owned(), "Beta segment.".to_owned()]);
}

#[test]
fn parse_trims_and_drops_empty_segments() {
    let s = r#"["  Padded segment.  ", "", "   ", "Real segment."]"#;
    let segments = parse_briefing_array(s).unwrap();
    assert_eq!(segments, vec!["Padded segment.".to_owned(), "Real segment.".to_owned()]);
}

#[test]
fn parse_fails_when_no_array() {
    assert!(parse_briefing_array("no brackets at all here").is_err());
}

#[test]
fn parse_fails_on_empty_array() {
    assert!(parse_briefing_array("[]").is_err());
}

#[test]
fn parse_fails_on_brackets_out_of_order() {
    assert!(parse_briefing_array("] first [ second").is_err());
}

#[test]
fn fallback_lists_episode_titles() {
    let eps = vec![
        ("Tech Pod".to_owned(), "AI Update".to_owned(), "desc".to_owned()),
        ("Sci Pod".to_owned(), "Mars Mission".to_owned(), "desc".to_owned()),
    ];
    let segments = fallback_segments(&eps);
    assert_eq!(segments.len(), 1);
    assert!(segments[0].contains("2 recent unplayed episodes"));
    assert!(segments[0].contains("AI Update (Tech Pod)"));
    assert!(segments[0].contains("Mars Mission (Sci Pod)"));
}

#[test]
fn fallback_singular_for_one_episode() {
    let eps = vec![("Pod".to_owned(), "Solo".to_owned(), "desc".to_owned())];
    let segments = fallback_segments(&eps);
    assert!(segments[0].contains("1 recent unplayed episode "));
    assert!(!segments[0].contains("episodes"));
}

#[test]
fn fallback_handles_empty_input() {
    let segments = fallback_segments(&[]);
    assert_eq!(segments.len(), 1);
    assert!(segments[0].contains("caught up"));
}
