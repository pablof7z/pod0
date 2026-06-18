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

// ── Issue 1: ElevenLabs fallback field lifecycle ──────────────────────────────

#[test]
fn apply_report_failed_clears_elevenlabs_tracking_fields() {
    // When a Failed report arrives for an in-flight ElevenLabs Speak,
    // apply_report clears the tracking fields so voice_report.rs can
    // dispatch the AvSpeech fallback and then reset state cleanly.
    let mut s = VoiceState {
        is_speaking: true,
        current_request_id: Some("el-req-1".into()),
        current_speak_text: Some("Hello world".into()),
        current_is_elevenlabs: true,
        ..VoiceState::default()
    };
    let changed = apply_report(
        &mut s,
        VoiceReport::Failed {
            request_id: "el-req-1".into(),
            error: "synthesis error".into(),
        },
    );
    assert!(changed);
    assert!(!s.is_speaking);
    // Tracking fields must be cleared so no second retry fires.
    assert!(!s.current_is_elevenlabs);
    assert!(s.current_speak_text.is_none());
}

#[test]
fn apply_report_failed_avspeech_does_not_set_is_elevenlabs() {
    // When the fallback AvSpeech Speak fails, current_is_elevenlabs must
    // stay false — no second ElevenLabs retry should be triggered.
    let mut s = VoiceState {
        is_speaking: true,
        current_request_id: Some("av-req-1".into()),
        current_speak_text: Some("Hello world".into()),
        current_is_elevenlabs: false,
        ..VoiceState::default()
    };
    let changed = apply_report(
        &mut s,
        VoiceReport::Failed {
            request_id: "av-req-1".into(),
            error: "playback error".into(),
        },
    );
    assert!(changed);
    assert!(
        !s.current_is_elevenlabs,
        "AVSpeech fallback must not re-trigger ElevenLabs"
    );
    assert!(s.current_speak_text.is_none());
}

// ── Issue 3: barge_in_text helper ────────────────────────────────────────────

#[test]
fn barge_in_text_empty_does_not_trigger() {
    let report = VoiceReport::TranscriptPartial { text: String::new() };
    assert!(
        barge_in_text(&report).is_none(),
        "empty partial must not trigger barge-in"
    );
}

#[test]
fn barge_in_text_whitespace_only_does_not_trigger() {
    let report = VoiceReport::TranscriptPartial { text: "   ".into() };
    assert!(
        barge_in_text(&report).is_none(),
        "whitespace-only partial must not trigger barge-in"
    );
}

#[test]
fn barge_in_text_non_empty_triggers() {
    let report = VoiceReport::TranscriptPartial { text: "hello".into() };
    assert_eq!(
        barge_in_text(&report),
        Some("hello"),
        "non-empty partial must trigger barge-in"
    );
}

#[test]
fn barge_in_text_non_partial_report_does_not_trigger() {
    let report = VoiceReport::ListeningStarted;
    assert!(
        barge_in_text(&report).is_none(),
        "non-partial report must not trigger barge-in"
    );
}
