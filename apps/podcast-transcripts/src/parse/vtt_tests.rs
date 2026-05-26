use super::*;

const SAMPLE: &str = "WEBVTT\n\n\
    00:00:00.000 --> 00:00:03.500\n\
    <v Host>Welcome to the show.\n\n\
    00:00:03.500 --> 00:00:07.250\n\
    <v Guest>Glad to be here.\n\n\
    00:00:07.250 --> 00:00:10.000\n\
    Plain narration with no speaker.\n";

#[test]
fn parses_three_entry_document() {
    let t = parse_vtt(SAMPLE, "ep-1", "https://example.com/t.vtt").unwrap();
    assert_eq!(t.entries.len(), 3);
    assert_eq!(t.entries[0].speaker.as_deref(), Some("Host"));
    assert_eq!(t.entries[0].text, "Welcome to the show.");
    assert_eq!(t.entries[1].speaker.as_deref(), Some("Guest"));
    assert!((t.entries[1].start_secs - 3.5).abs() < 1e-9);
    assert_eq!(t.entries[2].speaker, None);
    assert_eq!(t.kind, TranscriptKind::Vtt);
}

#[test]
fn rejects_input_without_header() {
    let err = parse_vtt("00:00:00.000 --> 00:00:01.000\nHi", "ep-1", "u").unwrap_err();
    assert_eq!(err, ParseError::MissingHeader);
}

#[test]
fn skips_note_blocks() {
    let input = "WEBVTT\n\nNOTE this is a note\n\n00:00:00.000 --> 00:00:01.000\nHi";
    let t = parse_vtt(input, "ep-1", "u").unwrap();
    assert_eq!(t.entries.len(), 1);
    assert_eq!(t.entries[0].text, "Hi");
}

#[test]
fn accepts_cue_settings_on_right_side() {
    let input = "WEBVTT\n\n00:00:00.000 --> 00:00:05.000 align:start\nHello";
    let t = parse_vtt(input, "ep-1", "u").unwrap();
    assert_eq!(t.entries.len(), 1);
    assert!((t.entries[0].end_secs - 5.0).abs() < 1e-9);
}

#[test]
fn strips_inline_tags() {
    let input = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\n<v Host>Hello <c.class>world</c>.";
    let t = parse_vtt(input, "ep-1", "u").unwrap();
    assert_eq!(t.entries[0].text, "Hello world.");
}
