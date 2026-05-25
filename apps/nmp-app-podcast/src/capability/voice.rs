//! Podcast-local voice capability contract — `nmp.voice.capability`.
//!
//! This is the schema the iOS executor (`Capabilities/Tts/{ElevenLabsAdapter,
//! AvSpeechAdapter}.swift`, landing in M8.C) implements and the Rust
//! `podcast-voice` crate (M8.B) drives. Rust serializes a [`VoiceCommand`];
//! iOS executes it against the active TTS provider (ElevenLabs WS streaming
//! by default; AVSpeechSynthesizer fallback) and sends a [`VoiceReport`]
//! back.
//!
//! ## Doctrine
//!
//! * **D7 — capabilities report, never decide.** iOS speaks exactly the
//!   text Rust hands it, with the voice Rust hands it. It never decides
//!   when to cut a `Speak` short on barge-in, never decides which voice
//!   to fall back to when ElevenLabs is unreachable, never picks the
//!   voice for an empty `voice_id`. Barge-in cancellation, fallback
//!   policy, and voice selection all live in `podcast-voice::manager`
//!   (M8.B).
//! * **D6 — error envelopes.** `Failed` carries an `error: String`
//!   payload; the capability never throws across the FFI.
//! * **D8 — bounded reactivity.** Status reports are one-per-event
//!   (Started, Finished, Failed, Stopped) — there is no per-frame
//!   audio-chunk surface here; raw bytes flow through
//!   `nmp.audio.capability` or directly to the OS audio engine inside
//!   the executor.
//!
//! ## Namespace
//!
//! The namespace string is `nmp.voice.capability` to match the existing
//! `nmp.audio.capability` / `nmp.download.capability` convention and the
//! active NMP feature-parity plan. (The canonical plan uses
//! `nmp.tts.capability`; M8.A's local
//! contract uses `nmp.voice.capability` to align with the `podcast-voice`
//! crate naming. M8.B/C will reconcile the namespace string against the
//! upstream canonical spec in a follow-up migration. The split here is
//! deliberately narrower so the iOS executor in M8.C has a stable target
//! to implement now without blocking on the cross-repo dependency.)
//!
//! ## Schema stability
//!
//! This is the M8.A skeleton — a two-enum Command/Report shape. The
//! canonical `nmp-core::capability::tts` uses a multi-enum
//! `Open`/`SendText`/`Cancel`/`Close` streaming-session
//! split (`AudioChunk{bytes}` events, etc.). When that lands in
//! `nostrmultiplatform`, M8.B/C will widen this contract or reconcile
//! against the canonical one in a follow-up migration. M8.A's shape
//! is sufficient for the voice turn loop (`Speak` → `Started` →
//! `Finished`/`Stopped`/`Failed`) and the barge-in cancellation policy
//! decision in Rust.

use serde::{Deserialize, Serialize};

/// Capability namespace string. Mirrors `AUDIO_CAPABILITY_NAMESPACE` /
/// `DOWNLOAD_CAPABILITY_NAMESPACE` so the iOS-side router can dispatch
/// by the same string the broader capability plan uses.
pub const VOICE_CAPABILITY_NAMESPACE: &str = "nmp.voice.capability";

// ---------------------------------------------------------------------------
// Rust → iOS: VoiceCommand
// ---------------------------------------------------------------------------

/// Commands Rust dispatches to the iOS voice capability.
///
/// Wire form is `serde`-tagged on `"type"` (`snake_case`):
///
/// ```text
/// {"type":"speak","text":"…","voice_id":"…","request_id":"…"}
/// {"type":"stop"}
/// {"type":"set_voice","voice_id":"…"}
/// ```
///
/// **D7:** these are *imperative* actions on the active TTS engine; the
/// iOS side runs each one and reports the resulting state. There is no
/// `decide`-flavoured command — every variant maps to a concrete TTS
/// call. Provider routing (ElevenLabs vs. AVSpeech), voice fallback,
/// and barge-in cancellation all live in Rust.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VoiceCommand {
    /// Synthesize and play `text`. The executor reports `Started{request_id}`
    /// as soon as audio begins playing and `Finished{request_id}` on
    /// natural completion.
    ///
    /// `voice_id` is optional: `None` uses the executor's currently
    /// configured voice (last `SetVoice` or built-in default). Empty
    /// strings are treated as `None`.
    ///
    /// `request_id` is caller-supplied so Rust can correlate the
    /// subsequent `Started` / `Finished` / `Failed` reports against
    /// the originating turn. The barge-in cancellation policy
    /// (`Stop` emitted when voiced-segment events fire mid-utterance)
    /// uses the live `request_id` from the most recent `Speak`.
    Speak {
        /// UTF-8 plain text — no SSML, no markdown. The Rust side normalises
        /// before sending; the executor speaks exactly what arrives.
        text: String,
        /// Optional voice id (provider-specific opaque string). `None`
        /// or an empty string falls back to the current configured voice.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        voice_id: Option<String>,
        /// Caller-supplied correlation id; echoed back in every report.
        request_id: String,
    },
    /// Cancel any in-flight `Speak` immediately. Idempotent: a no-op
    /// when nothing is speaking. The executor emits a single `Stopped`
    /// report once the active synthesis is torn down — even if the
    /// natural `Finished` would have arrived microseconds later.
    ///
    /// **D7 (barge-in):** Rust decides *when* to stop based on the
    /// `nmp.stt.capability` voiced-segment stream; this command just
    /// executes the decision.
    Stop,
    /// Set the active voice for subsequent `Speak` commands that don't
    /// specify their own `voice_id`. The executor stores the id and
    /// uses it on the next synthesis call.
    SetVoice { voice_id: String },
    /// Begin on-device speech recognition. The executor configures its
    /// audio engine + `SFSpeechRecognizer` and emits
    /// [`VoiceReport::ListeningStarted`] once the microphone is live.
    /// Recognition chunks arrive as [`VoiceReport::TranscriptPartial`];
    /// the final transcript on silence/stop is
    /// [`VoiceReport::TranscriptFinal`]. Idempotent — a no-op when
    /// recognition is already running.
    ///
    /// **D7:** the kernel decides *when* to start listening (voice-mode
    /// activate, end of turn, …); the executor just translates the
    /// command into an `AVAudioEngine` start.
    StartListening,
    /// Stop on-device speech recognition. The executor tears down the
    /// recognition request, flushes the buffered transcript as a final
    /// [`VoiceReport::TranscriptFinal`] (when non-empty), and emits
    /// [`VoiceReport::ListeningStopped`]. Idempotent.
    StopListening,
}

impl VoiceCommand {
    /// Convenience: construct a `Speak` command from owned strings.
    #[must_use]
    pub fn speak(
        text: impl Into<String>,
        voice_id: Option<String>,
        request_id: impl Into<String>,
    ) -> Self {
        Self::Speak {
            text: text.into(),
            voice_id,
            request_id: request_id.into(),
        }
    }

    /// Convenience: construct a `SetVoice` command.
    #[must_use]
    pub fn set_voice(voice_id: impl Into<String>) -> Self {
        Self::SetVoice {
            voice_id: voice_id.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// iOS → Rust: VoiceReport
// ---------------------------------------------------------------------------

/// Events the iOS voice capability sends back to Rust.
///
/// Wire form is `serde`-tagged on `"type"` (`snake_case`).
///
/// **D7:** these are *observations* of what the TTS engine actually did,
/// not invitations for Rust to decide something. The iOS side never
/// includes a "you should do X" field; the kernel projects the report
/// into voice-session state and emits any follow-up [`VoiceCommand`]
/// from its own state machine (e.g. issuing the next queued utterance
/// when `Finished` arrives).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VoiceReport {
    /// Synthesis began for `request_id`. Audio is now flowing to the
    /// output device.
    Started { request_id: String },
    /// Synthesis completed naturally for `request_id`. The kernel may
    /// emit the next queued utterance.
    Finished { request_id: String },
    /// Synthesis failed for `request_id`. `error` is a human-readable
    /// diagnostic (NSError `localizedDescription`, websocket close
    /// reason, or HTTP status). Retry / fallback policy lives in Rust.
    Failed { request_id: String, error: String },
    /// The executor honoured a `Stop` command and tore down the active
    /// synthesis. No `request_id` here — `Stop` is one-shot and the
    /// kernel already knows which request was live.
    Stopped,
    /// On-device speech recognition has begun: the microphone is live
    /// and the executor is forwarding audio frames to
    /// `SFSpeechRecognizer`. The kernel flips `voice.is_listening` to
    /// `true` on receipt.
    ListeningStarted,
    /// On-device speech recognition has stopped — either because the
    /// kernel sent `StopListening`, the recognizer emitted a final
    /// result, or an error tore the session down. The kernel flips
    /// `voice.is_listening` to `false` and clears any leftover partial
    /// transcript on receipt.
    ListeningStopped,
    /// Streaming partial recognition result. Fires every recognition
    /// chunk while listening. `text` is the *cumulative* best-guess so
    /// far (`SFSpeechRecognitionResult.bestTranscription`), not a delta
    /// — the kernel can render it directly without buffering.
    TranscriptPartial { text: String },
    /// Final transcript for the listening turn. Fires once on silence
    /// detection or an explicit `StopListening`. `text` is the
    /// committed best transcription; the kernel stores it (and clears
    /// the partial slot) before any follow-up action.
    TranscriptFinal { text: String },
    /// Capability-level error not tied to a specific `Speak` request
    /// (permission denial, audio-session preempt, recognizer unavailable
    /// in this locale, …). `message` is the human-readable diagnostic
    /// the kernel surfaces; retry policy lives in Rust.
    Error { message: String },
}

#[cfg(test)]
mod tests {
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
        // `skip_serializing_if = "Option::is_none"` keeps the wire tidy.
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
        assert_eq!(
            json,
            r#"{"type":"set_voice","voice_id":"rachel"}"#
        );
        let decoded: VoiceCommand = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn voice_report_started_serde_roundtrips() {
        let rep = VoiceReport::Started {
            request_id: "req-1".into(),
        };
        let json = serde_json::to_string(&rep).expect("encode");
        assert_eq!(json, r#"{"type":"started","request_id":"req-1"}"#);
        let decoded: VoiceReport = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, rep);
    }

    #[test]
    fn voice_report_finished_serde_roundtrips() {
        let rep = VoiceReport::Finished {
            request_id: "req-1".into(),
        };
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
        // Forward-compat: an older binary decoding a newer report ignores
        // fields it doesn't know about.
        let json = r#"{"type":"started","request_id":"req-1","future_field":42}"#;
        let decoded: VoiceReport = serde_json::from_str(json).expect("decode");
        assert_eq!(
            decoded,
            VoiceReport::Started {
                request_id: "req-1".into()
            }
        );
    }

    // ── STT + voice-mode extension (feature #42) ─────────────────────

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
        let rep = VoiceReport::TranscriptPartial {
            text: "hello world".into(),
        };
        let json = serde_json::to_string(&rep).expect("encode");
        assert!(json.contains("\"type\":\"transcript_partial\""));
        assert!(json.contains("\"text\":\"hello world\""));
        let decoded: VoiceReport = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, rep);
    }

    #[test]
    fn voice_report_transcript_final_round_trips() {
        let rep = VoiceReport::TranscriptFinal {
            text: "play the latest episode".into(),
        };
        let json = serde_json::to_string(&rep).expect("encode");
        assert!(json.contains("\"type\":\"transcript_final\""));
        let decoded: VoiceReport = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, rep);
    }

    #[test]
    fn voice_report_error_carries_message() {
        let rep = VoiceReport::Error {
            message: "speech recognition denied".into(),
        };
        let json = serde_json::to_string(&rep).expect("encode");
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("speech recognition denied"));
        let decoded: VoiceReport = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, rep);
    }
}
