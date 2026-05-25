//! Voice ActionModule — routes all `"podcast.voice.*"` dispatches.
//!
//! Swift encodes every voice action as `{"op":"<variant>", ...fields}`.
//! `#[serde(tag = "op", rename_all = "snake_case")]` maps the string
//! discriminator to the typed enum variant. The module's `execute` body
//! forwards the whole action as `ActorCommand::DispatchHostOp` so the
//! `PodcastHostOpHandler` (running on the actor thread) can dispatch the
//! matching [`crate::capability::VoiceCommand`] to the iOS voice
//! capability without the kernel naming podcast-domain nouns (D0).
//!
//! Variants:
//!
//! * `activate` / `deactivate` — voice-mode UI affordances (microphone
//!   button tap). Translate into `StartListening` / `StopListening`
//!   capability commands and flip `voice.is_listening` in the snapshot.
//! * `speak` / `stop` / `set_voice` — the M8.A TTS surface, kept on the
//!   same module so the iOS shell has one namespace to import. The
//!   payload types continue to live in [`super::voice`] for back-compat;
//!   this module is the dispatch entry point.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::ActorCommand;

/// Wire enum for all `"podcast.voice"` namespace actions.
///
/// JSON shape is `{"op":"<variant>", ...fields}`. Snake-case variant
/// names match the kernel-side dispatch arms in
/// `host_op_handler::handle_voice_action`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum VoiceAction {
    /// Enter voice mode: begin on-device STT and flip
    /// `voice.is_listening` to `true` in the snapshot. The host op
    /// handler emits a `VoiceCommand::StartListening` to the iOS
    /// executor; partial transcripts flow back as
    /// `VoiceReport::TranscriptPartial`.
    Activate,
    /// Exit voice mode: stop on-device STT and flip
    /// `voice.is_listening` back to `false`. The host op handler emits
    /// a `VoiceCommand::StopListening` to the iOS executor.
    Deactivate,
    /// Synthesize and play `text` via the active TTS provider.
    /// Mirrors [`super::voice::SpeakAction`] for back-compat — the
    /// host op handler mints a `request_id` and dispatches a
    /// `VoiceCommand::Speak`.
    Speak {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        voice_id: Option<String>,
    },
    /// Cancel any in-flight `Speak`. Idempotent.
    Stop,
    /// Change the active TTS voice for subsequent `Speak` actions that
    /// don't carry their own `voice_id`.
    SetVoice { voice_id: String },
}

/// Single action module for the whole `"podcast.voice"` namespace.
///
/// `execute` serializes the typed [`VoiceAction`] back to JSON and hands
/// it to the actor as [`ActorCommand::DispatchHostOp`]. The installed
/// `PodcastHostOpHandler` deserializes it, dispatches the matching
/// `VoiceCommand` to the iOS executor, and mutates the handle's
/// `voice_state` so the next snapshot reflects the transition (D7 —
/// kernel decides, iOS reports).
pub struct VoiceActionModule;

impl ActionModule for VoiceActionModule {
    const NAMESPACE: &'static str = "podcast.voice";

    type Action = VoiceAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        let action_json = serde_json::to_string(&action).map_err(|e| e.to_string())?;
        send(ActorCommand::DispatchHostOp {
            action_json,
            correlation_id: correlation_id.to_owned(),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activate_action_round_trips() {
        let action = VoiceAction::Activate;
        let json = serde_json::to_string(&action).expect("encode");
        assert_eq!(json, r#"{"op":"activate"}"#);
        let decoded: VoiceAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn deactivate_action_round_trips() {
        let action = VoiceAction::Deactivate;
        let json = serde_json::to_string(&action).expect("encode");
        assert_eq!(json, r#"{"op":"deactivate"}"#);
        let decoded: VoiceAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn speak_action_round_trips_with_voice_id() {
        let action = VoiceAction::Speak {
            text: "hello world".into(),
            voice_id: Some("rachel".into()),
        };
        let json = serde_json::to_string(&action).expect("encode");
        assert!(json.contains(r#""op":"speak""#));
        assert!(json.contains(r#""text":"hello world""#));
        assert!(json.contains(r#""voice_id":"rachel""#));
        let decoded: VoiceAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, action);
    }

    #[test]
    fn speak_action_omits_none_voice_id() {
        let action = VoiceAction::Speak {
            text: "hi".into(),
            voice_id: None,
        };
        let json = serde_json::to_string(&action).expect("encode");
        assert_eq!(json, r#"{"op":"speak","text":"hi"}"#);
    }

    #[test]
    fn stop_and_set_voice_round_trip() {
        assert_eq!(
            serde_json::to_string(&VoiceAction::Stop).expect("encode"),
            r#"{"op":"stop"}"#
        );
        let sv = VoiceAction::SetVoice {
            voice_id: "rachel".into(),
        };
        let json = serde_json::to_string(&sv).expect("encode");
        assert_eq!(json, r#"{"op":"set_voice","voice_id":"rachel"}"#);
        let decoded: VoiceAction = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, sv);
    }

    #[test]
    fn execute_emits_dispatch_host_op() {
        let action = VoiceAction::Activate;
        let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
        VoiceActionModule::execute(action, "corr-1", &|cmd| {
            commands.lock().unwrap().push(cmd);
        })
        .expect("execute ok");
        let commands = commands.into_inner().unwrap();
        assert_eq!(commands.len(), 1);
        let ActorCommand::DispatchHostOp {
            action_json,
            correlation_id,
        } = &commands[0]
        else {
            panic!("expected DispatchHostOp");
        };
        assert_eq!(correlation_id, "corr-1");
        let v: serde_json::Value = serde_json::from_str(action_json).expect("json");
        assert_eq!(v["op"], "activate");
    }
}
