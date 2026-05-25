//! Voice action ids + payloads — the shell → kernel surface for
//! `podcast.voice.*`.
//!
//! The Rust voice turn loop (lands in `podcast-voice` at M8.B) drives
//! the iOS `nmp.voice.capability` executor via `VoiceCommand` (see
//! [`crate::capability::voice`]). These action ids are the *external*
//! shell → kernel surface — UI affordances ("speak this", "stop
//! speaking", "switch voice") that the kernel translates into the
//! right capability command. M8.B/C will wire the action modules; M8.A
//! only fixes the wire shape so the Swift bridge has a contract to
//! encode against.
//!
//! ## Wire shape
//!
//! ```text
//! podcast.voice.speak     — SpeakAction     { text, voice_id? }
//! podcast.voice.stop      — StopVoiceAction
//! podcast.voice.set_voice — SetVoiceAction  { voice_id }
//! ```

use serde::{Deserialize, Serialize};

/// `podcast.voice.speak` — synthesize and play `text` through the active
/// TTS provider. The kernel mints a `request_id`, dispatches
/// `VoiceCommand::Speak`, and emits the in-flight session into the
/// `voice` projection on the next snapshot tick.
pub const ACTION_VOICE_SPEAK: &str = "podcast.voice.speak";

/// `podcast.voice.stop` — cancel any in-flight TTS immediately. Used by
/// barge-in (kernel decides; this is the manual UI affordance) and by
/// the user's "stop speaking" button.
pub const ACTION_VOICE_STOP: &str = "podcast.voice.stop";

/// `podcast.voice.set_voice` — change the active voice for subsequent
/// `Speak` actions that don't specify their own `voice_id`.
pub const ACTION_VOICE_SET_VOICE: &str = "podcast.voice.set_voice";

/// Payload for [`ACTION_VOICE_SPEAK`].
///
/// `voice_id` is optional: `None` uses the executor's currently
/// configured voice (last `SetVoice` or built-in default). The kernel
/// mints the `request_id` for the underlying
/// [`crate::capability::voice::VoiceCommand::Speak`] — it is not part
/// of the action payload so the iOS UI doesn't have to invent
/// correlation ids.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SpeakAction {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_id: Option<String>,
}

/// Payload for [`ACTION_VOICE_STOP`]. Empty — stop always targets the
/// in-flight TTS request.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct StopVoiceAction;

/// Payload for [`ACTION_VOICE_SET_VOICE`].
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SetVoiceAction {
    pub voice_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_action_ids_match_documented_strings() {
        assert_eq!(ACTION_VOICE_SPEAK, "podcast.voice.speak");
        assert_eq!(ACTION_VOICE_STOP, "podcast.voice.stop");
        assert_eq!(ACTION_VOICE_SET_VOICE, "podcast.voice.set_voice");
    }

    #[test]
    fn speak_action_round_trips_with_voice_id() {
        let a = SpeakAction {
            text: "hello world".into(),
            voice_id: Some("rachel".into()),
        };
        let json = serde_json::to_string(&a).expect("encode");
        assert!(json.contains("\"text\":\"hello world\""));
        assert!(json.contains("\"voice_id\":\"rachel\""));
        let decoded: SpeakAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }

    #[test]
    fn speak_action_omits_none_voice_id() {
        let a = SpeakAction {
            text: "hi".into(),
            voice_id: None,
        };
        let json = serde_json::to_string(&a).expect("encode");
        assert_eq!(json, r#"{"text":"hi"}"#);
        let decoded: SpeakAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }

    #[test]
    fn stop_voice_action_is_unit_struct() {
        let a = StopVoiceAction;
        let json = serde_json::to_string(&a).expect("encode");
        assert_eq!(json, "null");
    }

    #[test]
    fn set_voice_action_round_trips() {
        let a = SetVoiceAction {
            voice_id: "rachel".into(),
        };
        let json = serde_json::to_string(&a).expect("encode");
        assert_eq!(json, r#"{"voice_id":"rachel"}"#);
        let decoded: SetVoiceAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, a);
    }
}
