use super::*;

#[test]
fn apply_report_started_flips_speaking_and_sets_request_id() {
    let mut s = VoiceState::default();
    let changed = apply_report(
        &mut s,
        VoiceReport::Started {
            request_id: "req-1".into(),
        },
    );
    assert!(changed);
    assert!(s.is_speaking);
    assert_eq!(s.current_request_id.as_deref(), Some("req-1"));
}

#[test]
fn apply_report_finished_clears_speaking() {
    let mut s = VoiceState {
        is_speaking: true,
        current_request_id: Some("req-1".into()),
        ..VoiceState::default()
    };
    let changed = apply_report(
        &mut s,
        VoiceReport::Finished {
            request_id: "req-1".into(),
        },
    );
    assert!(changed);
    assert!(!s.is_speaking);
    assert!(s.current_request_id.is_none());
}

#[test]
fn apply_report_listening_started_flips_listening() {
    let mut s = VoiceState::default();
    assert!(apply_report(&mut s, VoiceReport::ListeningStarted));
    assert!(s.is_listening);
}

#[test]
fn apply_report_listening_stopped_clears_partial() {
    let mut s = VoiceState {
        is_listening: true,
        partial_transcript: Some("hello".into()),
        ..VoiceState::default()
    };
    assert!(apply_report(&mut s, VoiceReport::ListeningStopped));
    assert!(!s.is_listening);
    assert!(s.partial_transcript.is_none());
}

#[test]
fn apply_report_transcript_partial_updates_caption() {
    let mut s = VoiceState {
        is_listening: true,
        ..VoiceState::default()
    };
    assert!(apply_report(
        &mut s,
        VoiceReport::TranscriptPartial {
            text: "play the".into(),
        }
    ));
    assert_eq!(s.partial_transcript.as_deref(), Some("play the"));
}

#[test]
fn apply_report_transcript_final_clears_partial_and_sets_response() {
    let mut s = VoiceState {
        is_listening: true,
        partial_transcript: Some("play the".into()),
        ..VoiceState::default()
    };
    assert!(apply_report(
        &mut s,
        VoiceReport::TranscriptFinal {
            text: "play the latest".into(),
        }
    ));
    assert!(s.partial_transcript.is_none());
    assert_eq!(s.last_response.as_deref(), Some("play the latest"));
}

#[test]
fn apply_report_error_surfaces_message() {
    let mut s = VoiceState::default();
    assert!(apply_report(
        &mut s,
        VoiceReport::Error {
            message: "denied".into(),
        }
    ));
    assert!(s.last_response.as_deref().unwrap().contains("denied"));
}

#[test]
fn apply_report_returns_false_on_noop() {
    let mut s = VoiceState::default();
    let changed = apply_report(&mut s, VoiceReport::Stopped);
    assert!(!changed);
}
