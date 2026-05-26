use super::*;

const SAMPLE: &str = "1\n\
    00:00:00,000 --> 00:00:03,500\n\
    Tim Ferriss: Welcome back to the show.\n\n\
    2\n\
    00:00:03,500 --> 00:00:07,250\n\
    [Guest]: Glad to be here.\n\n\
    3\n\
    00:00:07,250 --> 00:00:10,000\n\
    Just a narration line.\n";

#[test]
fn parses_three_entry_srt() {
    let t = parse_srt(SAMPLE, "ep-1", "https://example.com/t.srt").unwrap();
    assert_eq!(t.entries.len(), 3);
    assert_eq!(t.entries[0].speaker.as_deref(), Some("Tim Ferriss"));
    assert_eq!(t.entries[0].text, "Welcome back to the show.");
    assert_eq!(t.entries[1].speaker.as_deref(), Some("Guest"));
    assert_eq!(t.entries[1].text, "Glad to be here.");
    assert_eq!(t.entries[2].speaker, None);
    assert!((t.entries[2].end_secs - 10.0).abs() < 1e-9);
    assert_eq!(t.kind, TranscriptKind::Srt);
}

#[test]
fn rejects_empty_input() {
    assert_eq!(parse_srt("   ", "ep-1", "u"), Err(ParseError::Empty));
}

#[test]
fn does_not_eat_colons_in_body() {
    let input = "1\n00:00:00,000 --> 00:00:01,000\nYeah, well: I think so.\n";
    let t = parse_srt(input, "ep-1", "u").unwrap();
    assert_eq!(t.entries[0].speaker, None);
    assert_eq!(t.entries[0].text, "Yeah, well: I think so.");
}

#[test]
fn handles_chevron_prefix() {
    let input = "1\n00:00:00,000 --> 00:00:01,000\n>> Host: hello there\n";
    let t = parse_srt(input, "ep-1", "u").unwrap();
    assert_eq!(t.entries[0].speaker.as_deref(), Some("Host"));
    assert_eq!(t.entries[0].text, "hello there");
}

#[test]
fn rejects_label_with_url() {
    assert!(!is_plausible_speaker_label("https"));
    assert!(!is_plausible_speaker_label("Yeah, well"));
    assert!(is_plausible_speaker_label("Tim Ferriss"));
    assert!(is_plausible_speaker_label("Dr. Huberman"));
    assert!(is_plausible_speaker_label("PETER ATTIA"));
}
