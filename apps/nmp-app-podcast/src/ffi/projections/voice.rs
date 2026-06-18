use serde::{Deserialize, Serialize};

/// Snapshot of the voice (TTS) session surfaced via
/// [`super::snapshot::PodcastUpdate::voice`].
///
/// Mirrors `crate::capability::voice::VoiceCommand` / `VoiceReport`
/// state on the kernel side: `is_speaking` flips to `true` when the
/// executor reports `Started`, back to `false` on `Finished` / `Failed`
/// / `Stopped`. `current_request_id` is the in-flight TTS correlation
/// id (matching the legacy Swift `VoiceTurn` request id);
/// `current_voice_id` is the active voice the user / agent selected.
///
/// `current_request_id` and `current_voice_id` are `Option` because
/// the UI may need to render "speaking but voice id not yet bound"
/// (mid-fallback) or "idle but voice id remembered" (between turns) —
/// surfacing both fields independently saves a re-derivation in Swift.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct VoiceState {
    /// `true` while a `Speak` is in flight (between `Started` and the
    /// terminal `Finished` / `Failed` / `Stopped`).
    pub is_speaking: bool,
    /// `true` while on-device speech recognition is running (between
    /// `ListeningStarted` and `ListeningStopped`). Drives the
    /// pulsing-microphone affordance in `VoiceModeView`.
    #[serde(default)]
    pub is_listening: bool,
    /// Correlation id of the in-flight `Speak`, mirrored from the
    /// `VoiceCommand::Speak.request_id`. `None` when nothing is in
    /// flight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_request_id: Option<String>,
    /// The voice id the executor is currently configured to use.
    /// Set by the most recent `SetVoice` or by the explicit
    /// `voice_id` on a `Speak`. `None` until the user picks one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_voice_id: Option<String>,
    /// Streaming best-guess transcript while `is_listening == true`.
    /// Updated on every [`crate::capability::VoiceReport::TranscriptPartial`]
    /// report; cleared back to `None` on `TranscriptFinal` /
    /// `ListeningStopped`. The UI binds the voice-mode caption to this
    /// field so chunked recognition results render with no extra
    /// buffering on the Swift side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_transcript: Option<String>,
    /// Most recent committed transcript or assistant reply the UI
    /// surfaces under the voice-mode orb. Updated by the kernel on
    /// `TranscriptFinal` (the user said this) or on a `Speak` action
    /// (the assistant said this). `None` between sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_response: Option<String>,
    /// Internal: text of the in-flight ElevenLabs Speak (for AVSpeech fallback dispatch).
    /// `#[serde(skip)]` — not part of the snapshot surface.
    #[serde(skip)]
    pub(crate) current_speak_text: Option<String>,
    /// Internal: whether the current in-flight Speak is an ElevenLabs request.
    /// `false` once the AVSpeech fallback is dispatched, preventing a second retry.
    /// `#[serde(skip)]` — not part of the snapshot surface.
    #[serde(skip)]
    pub(crate) current_is_elevenlabs: bool,
}
