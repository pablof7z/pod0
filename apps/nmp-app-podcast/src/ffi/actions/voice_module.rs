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
use nmp_core::actor::ActorCommand;

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
    const NAMESPACE: nmp_core::substrate::DeclaredActionNamespace =
        nmp_core::substrate::DeclaredActionNamespace::app_owned("podcast.voice");

    type Action = VoiceAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        &self,
        _ctx: &nmp_core::substrate::ActionContext,
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        crate::ffi::actions::dispatch_host_op(Self::NAMESPACE.as_str(), &action, correlation_id, send)
    }

    fn decode_payload(
        bytes: &[u8],
    ) -> Option<Result<Self::Action, nmp_core::substrate::ActionPayloadDecodeError>> {
        crate::action_payload::decode_podcast_payload(bytes)
    }
}

#[cfg(test)]
#[path = "voice_module_tests.rs"]
mod tests;
