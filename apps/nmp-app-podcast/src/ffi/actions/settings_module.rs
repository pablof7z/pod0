//! Settings `ActionModule` — routes `"podcast.settings.*"` dispatches.
//!
//! Swift encodes every settings action as `{"op":"<variant>", ...fields}`.
//! The `#[serde(tag = "op", rename_all = "snake_case")]` discriminator
//! maps the string `op` value to the enum variant. The module's
//! `execute` body forwards the whole action as
//! `ActorCommand::DispatchHostOp` so the `PodcastHostOpHandler`
//! (running on the actor thread) can mutate `PodcastStore` settings +
//! mirror the changed value into `PlayerActor` where relevant.

use serde::{Deserialize, Serialize};

use nmp_core::substrate::ActionModule;
use nmp_core::ActorCommand;

/// Wire enum for all `"podcast.settings"` namespace actions.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SettingsAction {
    /// Set the user-facing auto-skip-ads toggle. The store persists
    /// the flag; the active `PlayerActor` is updated in lock-step so
    /// the next `Playing` report sees the new value without waiting
    /// for a `play` action.
    SetAutoSkipAds { enabled: bool },
    /// Update both skip intervals. Clamped server-side to `[1, 120]` seconds.
    /// iOS dispatches this when the user changes the skip interval in Settings.
    SetSkipIntervals { forward_secs: f64, backward_secs: f64 },
    /// Toggle auto-play-next: when `true`, the kernel auto-advances to
    /// the next queued episode on natural `ItemEnd`.
    SetAutoPlayNext { enabled: bool },
    /// Toggle auto-mark-played: when `true`, the kernel marks the episode
    /// listened on natural `ItemEnd`.
    SetAutoMarkPlayedAtEnd { enabled: bool },
    /// Update both headphone gesture action strings. The raw values must
    /// match a `HeadphoneGestureAction` case on the iOS side.
    SetHeadphoneGestureActions { double_tap: String, triple_tap: String },
    /// Set the default playback rate. Clamped server-side to `[0.5, 3.0]`.
    SetDefaultPlaybackRate { rate: f64 },
    /// Toggle delete-downloaded-file-after-played. When `true`, the kernel
    /// deletes the local audio file after marking the episode played.
    SetAutoDeleteDownloadsAfterPlayed { enabled: bool },
    /// Set both the model ID and name for initial agent chat. Carried atomically
    /// in one action so id and name update together and the guard fires when
    /// either value changes.
    SetAgentInitialModel { model: String, model_name: String },
    /// Set both the model ID and name for agent thinking/planning. Atomic update.
    SetAgentThinkingModel { model: String, model_name: String },
    /// Set both the model ID and name for memory compilation. Atomic update.
    SetMemoryCompilationModel { model: String, model_name: String },
    /// Set both the model ID and name for wiki synthesis. Atomic update.
    SetWikiModel { model: String, model_name: String },
    /// Set both the model ID and name for episode categorization. Atomic update.
    SetCategorizationModel { model: String, model_name: String },
    /// Set both the model ID and name for chapter compilation. Atomic update.
    SetChapterCompilationModel { model: String, model_name: String },
    /// Set both the model ID and name for embeddings generation. Atomic update.
    SetEmbeddingsModel { model: String, model_name: String },
    /// Set both the model ID and name for image generation. Atomic update.
    SetImageGenerationModel { model: String, model_name: String },
    /// Set the reranker-enabled toggle.
    SetRerankerEnabled { enabled: bool },
    /// Set OpenRouter credential metadata (source, key_id, key_label, connected_at).
    /// Secrets stay in Keychain; only metadata is persisted.
    SetOpenRouterCredential {
        source: String,
        key_id: Option<String>,
        key_label: Option<String>,
        connected_at: Option<i64>,
    },
    /// Set Ollama credential metadata (source, key_id, key_label, connected_at).
    /// Secrets stay in Keychain; only metadata is persisted.
    SetOllamaCredential {
        source: String,
        key_id: Option<String>,
        key_label: Option<String>,
        connected_at: Option<i64>,
    },
    /// Set Ollama chat endpoint URL for LLM inference.
    SetOllamaChatUrl { url: String },
    /// Set ElevenLabs credential metadata (source, key_id, key_label, connected_at).
    /// Secrets stay in Keychain; only metadata is persisted.
    SetElevenLabsCredential {
        source: String,
        key_id: Option<String>,
        key_label: Option<String>,
        connected_at: Option<i64>,
    },
}

/// Action module for the `"podcast.settings"` namespace.
pub struct SettingsActionModule;

impl ActionModule for SettingsActionModule {
    const NAMESPACE: &'static str = "podcast.settings";

    type Action = SettingsAction;

    fn is_async_completing() -> bool {
        false
    }

    fn execute(
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        let action_json =
            serde_json::to_string(&action).map_err(|e| e.to_string())?;
        send(ActorCommand::DispatchHostOp {
            action_json,
            correlation_id: correlation_id.to_owned(),
        });
        Ok(())
    }
}

#[cfg(test)]
#[path = "settings_module_tests.rs"]
mod tests;
