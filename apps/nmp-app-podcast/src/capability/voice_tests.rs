//! Tests for [`super::voice`] — VoiceCommand / VoiceReport serde coverage.
//!
//! Extracted from `voice.rs` to keep that file under the 500-line hard limit.

use super::*;

#[test]
fn namespace_matches_documented_string() {
    assert_eq!(VOICE_CAPABILITY_NAMESPACE, "nmp.voice.capability");
}

#[test]
fn voice_command_speak_serde_roundtrips_with_voice_id() {
    let cmd = VoiceCommand::speak("hello world", Some("rachel".into()), "req-1");
    let json = serde_json::to_string(&cmd).expect("encode");
    assert!(json.contains("\"type\":\"speak\""));
    assert!(json.contains("\"text\":\"hello world\""));
    assert!(json.contains("\"voice_id\":\"rachel\""));
    assert!(json.contains("\"request_id\":\"req-1\""));
    let decoded: VoiceCommand = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, cmd);
}

#[test]
fn voice_command_speak_omits_none_voice_id() {
    let cmd = VoiceCommand::speak("hi", None, "req-2");
    let json = serde_json::to_string(&cmd).expect("encode");
    assert!(!json.contains("voice_id"));
    let decoded: VoiceCommand = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, cmd);
}

#[test]
fn voice_command_stop_has_no_payload() {
    assert_eq!(
        serde_json::to_string(&VoiceCommand::Stop).expect("encode"),
        r#"{"type":"stop"}"#
    );
}

#[test]
fn voice_command_set_voice_serde_roundtrips() {
    let cmd = VoiceCommand::set_voice("rachel");
    let json = serde_json::to_string(&cmd).expect("encode");
    assert_eq!(json, r#"{"type":"set_voice","voice_id":"rachel"}"#);
    let decoded: VoiceCommand = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, cmd);
}

#[test]
fn voice_report_started_serde_roundtrips() {
    let rep = VoiceReport::Started { request_id: "req-1".into() };
    let json = serde_json::to_string(&rep).expect("encode");
    assert_eq!(json, r#"{"type":"started","request_id":"req-1"}"#);
    let decoded: VoiceReport = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, rep);
}

#[test]
fn voice_report_finished_serde_roundtrips() {
    let rep = VoiceReport::Finished { request_id: "req-1".into() };
    let json = serde_json::to_string(&rep).expect("encode");
    assert_eq!(json, r#"{"type":"finished","request_id":"req-1"}"#);
    let decoded: VoiceReport = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, rep);
}

#[test]
fn voice_report_failed_carries_request_id_and_error() {
    let rep = VoiceReport::Failed {
        request_id: "req-1".into(),
        error: "transport: timeout".into(),
    };
    let json = serde_json::to_string(&rep).expect("encode");
    assert!(json.contains("\"type\":\"failed\""));
    assert!(json.contains("transport: timeout"));
    let decoded: VoiceReport = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, rep);
}

#[test]
fn voice_report_stopped_has_no_payload() {
    assert_eq!(
        serde_json::to_string(&VoiceReport::Stopped).expect("encode"),
        r#"{"type":"stopped"}"#
    );
}

#[test]
fn voice_report_unknown_field_tolerated() {
    let json = r#"{"type":"started","request_id":"req-1","future_field":42}"#;
    let decoded: VoiceReport = serde_json::from_str(json).expect("decode");
    assert_eq!(decoded, VoiceReport::Started { request_id: "req-1".into() });
}

#[test]
fn voice_command_start_listening_serializes_as_unit() {
    assert_eq!(
        serde_json::to_string(&VoiceCommand::StartListening).expect("encode"),
        r#"{"type":"start_listening"}"#
    );
    let decoded: VoiceCommand =
        serde_json::from_str(r#"{"type":"start_listening"}"#).expect("decode");
    assert_eq!(decoded, VoiceCommand::StartListening);
}

#[test]
fn voice_command_stop_listening_serializes_as_unit() {
    assert_eq!(
        serde_json::to_string(&VoiceCommand::StopListening).expect("encode"),
        r#"{"type":"stop_listening"}"#
    );
    let decoded: VoiceCommand =
        serde_json::from_str(r#"{"type":"stop_listening"}"#).expect("decode");
    assert_eq!(decoded, VoiceCommand::StopListening);
}

#[test]
fn voice_report_listening_started_stopped_round_trip() {
    for (variant, tag) in [
        (VoiceReport::ListeningStarted, "listening_started"),
        (VoiceReport::ListeningStopped, "listening_stopped"),
    ] {
        let json = serde_json::to_string(&variant).expect("encode");
        assert_eq!(json, format!(r#"{{"type":"{tag}"}}"#));
        let decoded: VoiceReport = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, variant);
    }
}

#[test]
fn voice_report_transcript_partial_round_trips() {
    let rep = VoiceReport::TranscriptPartial { text: "hello world".into() };
    let json = serde_json::to_string(&rep).expect("encode");
    assert!(json.contains("\"type\":\"transcript_partial\""));
    assert!(json.contains("\"text\":\"hello world\""));
    let decoded: VoiceReport = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, rep);
}

#[test]
fn voice_report_transcript_final_round_trips() {
    let rep = VoiceReport::TranscriptFinal { text: "play the latest episode".into() };
    let json = serde_json::to_string(&rep).expect("encode");
    assert!(json.contains("\"type\":\"transcript_final\""));
    let decoded: VoiceReport = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, rep);
}

#[test]
fn voice_report_error_carries_message() {
    let rep = VoiceReport::Error { message: "speech recognition denied".into() };
    let json = serde_json::to_string(&rep).expect("encode");
    assert!(json.contains("\"type\":\"error\""));
    assert!(json.contains("speech recognition denied"));
    let decoded: VoiceReport = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, rep);
}
