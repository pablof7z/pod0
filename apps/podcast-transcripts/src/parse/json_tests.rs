use super::*;

const SAMPLE: &str = r#"{
    "version": "1.0.0",
    "language": "en-GB",
    "segments": [
        {"speaker": "Host", "startTime": 0.0, "endTime": 3.4, "body": "Welcome back."},
        {"speaker": "Guest", "startTime": 3.4, "endTime": 7.0, "body": "Thanks for having me.",
         "words": [
             {"word": "Thanks", "startTime": 3.4, "endTime": 3.8},
             {"word": "for", "startTime": 3.8, "endTime": 4.0}
         ]},
        {"startTime": "7.0", "endTime": "10.0", "text": "Stringified times work too."}
    ]
}"#;

#[test]
fn parses_three_segment_doc() {
    let t = parse_podcasting_json(SAMPLE.as_bytes(), "ep-1", "https://x/").unwrap();
    assert_eq!(t.entries.len(), 3);
    assert_eq!(t.entries[0].speaker.as_deref(), Some("Host"));
    assert_eq!(t.entries[1].words.as_ref().unwrap().len(), 2);
    assert_eq!(t.entries[2].text, "Stringified times work too.");
    assert!((t.entries[2].start_secs - 7.0).abs() < 1e-9);
    assert_eq!(t.kind, TranscriptKind::Json);
    assert_eq!(t.language, "en-GB");
}

#[test]
fn rejects_missing_segments() {
    let bytes = br#"{"version":"1.0.0"}"#;
    assert_eq!(
        parse_podcasting_json(bytes, "ep-1", "u"),
        Err(ParseError::MissingSegments)
    );
}

#[test]
fn rejects_invalid_json() {
    let bytes = b"{not json";
    assert!(matches!(
        parse_podcasting_json(bytes, "ep-1", "u"),
        Err(ParseError::InvalidJson(_))
    ));
}

#[test]
fn defaults_language_when_missing() {
    let bytes = br#"{"segments":[]}"#;
    let t = parse_podcasting_json(bytes, "ep-1", "u").unwrap();
    assert_eq!(t.language, "en-US");
    assert!(t.entries.is_empty());
}
