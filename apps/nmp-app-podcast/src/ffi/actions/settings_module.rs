//! Settings `ActionModule` — routes `"podcast.settings.*"` dispatches.
//!
//! Platform hosts encode every settings action as
//! `{"op":"<variant>", ...fields}`. The
//! `#[serde(tag = "op", rename_all = "snake_case")]` discriminator
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
    SetSkipIntervals {
        forward_secs: f64,
        backward_secs: f64,
    },
    /// Toggle auto-play-next: when `true`, the kernel auto-advances to
    /// the next queued episode on natural `ItemEnd`.
    SetAutoPlayNext { enabled: bool },
    /// Toggle auto-mark-played: when `true`, the kernel marks the episode
    /// listened on natural `ItemEnd`.
    SetAutoMarkPlayedAtEnd { enabled: bool },
    /// Update both headphone gesture action strings. The raw values must
    /// match a `HeadphoneGestureAction` case on the iOS side.
    SetHeadphoneGestureActions {
        double_tap: String,
        triple_tap: String,
    },
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
    /// Set AssemblyAI credential metadata (source, key_id, key_label, connected_at).
    /// Secrets stay in platform secure storage; only metadata is persisted.
    SetAssemblyAiCredential {
        source: String,
        key_id: Option<String>,
        key_label: Option<String>,
        connected_at: Option<i64>,
    },
    /// Set Perplexity credential metadata (source, key_id, key_label, connected_at).
    /// Secrets stay in platform secure storage; only metadata is persisted.
    SetPerplexityCredential {
        source: String,
        key_id: Option<String>,
        key_label: Option<String>,
        connected_at: Option<i64>,
    },
    /// Set the STT provider selection.
    SetSttProvider { provider: String },
    /// Report which STT providers currently have an API key in platform
    /// secure storage. `providers` is the full present-set of STT-provider raw
    /// values (`"elevenlabs_scribe"`, `"assemblyai"`, `"openrouter_whisper"`);
    /// the kernel replaces its mirror verbatim. Secrets never cross the FFI —
    /// only presence. Platform hosts dispatch this on launch and whenever a
    /// key is saved or deleted so the kernel-owned STT fallback policy stays
    /// current.
    SetSttKeysPresent { providers: Vec<String> },
    /// Set the OpenRouter Whisper model string.
    SetOpenRouterWhisperModel { model: String },
    /// Set the AssemblyAI STT model string.
    SetAssemblyAiSttModel { model: String },
    /// Set both ElevenLabs STT and TTS models. Atomic update.
    SetElevenLabsModels {
        stt_model: String,
        tts_model: String,
    },
    /// Set both ElevenLabs voice ID and name. Atomic update.
    SetElevenLabsVoice {
        voice_id: String,
        voice_name: String,
    },
    /// Set the Blossom server URL.
    SetBlossomServerUrl { url: String },
    /// Set the YouTube extractor URL (optional).
    SetYoutubeExtractorUrl { url: Option<String> },
    /// Set the local on-device LLM model ID (optional). When set, this dominates
    /// all other provider selections in the LLM factory.
    SetLocalModel { model_id: Option<String> },
    /// Toggle wiki auto-generation on transcript ingest.
    SetWikiAutoGenerateOnTranscriptIngest { enabled: bool },
    /// Toggle auto-ingest of publisher-provided transcripts.
    SetAutoIngestPublisherTranscripts { enabled: bool },
    /// Toggle auto-fallback to Scribe when publisher transcript ingestion fails.
    SetAutoFallbackToScribe { enabled: bool },
    /// Toggle local notifications for new episodes.
    SetNotifyOnNewEpisodes { enabled: bool },
    /// Set the Nostr-enabled toggle.
    SetNostrEnabled { enabled: bool },
    /// Set the Nostr relay URL.
    SetNostrRelayUrl { url: String },
    /// Set Nostr profile metadata (name, about, picture).
    SetNostrProfile {
        name: String,
        about: String,
        picture: String,
    },
    /// Set the Nostr public key hex (read-only, for projection only).
    SetNostrPublicKeyHex { hex: Option<String> },
    /// Add (or upsert the role of) a configured app relay. `role` is one of
    /// `read` | `write` | `both` | `indexer` (optionally comma-joined, e.g.
    /// `both,indexer`); the kernel normalizes and validates it server-side and
    /// surfaces a toast on an invalid URL/role. Drives the iOS App Relays
    /// editor. Unlike the other settings ops this does NOT route through the
    /// host-op handler — relay state lives in the kernel `AppRelaySlot`, not
    /// `PodcastStore`, so `execute` emits `ActorCommand::AddRelay` directly.
    AddRelay { url: String, role: String },
    /// Remove a configured app relay by URL. Idempotent: removing a URL that
    /// is not present is a no-op. Emits `ActorCommand::RemoveRelay` directly.
    RemoveRelay { url: String },
    /// Change the NIP-65 role of an already-configured relay. There is no
    /// dedicated kernel command — `add_relay` upserts on URL (replacing the
    /// role of an existing row), so this emits `ActorCommand::AddRelay` with
    /// the new role, exactly like `AddRelay`.
    SetRelayRole { url: String, role: String },
    /// Set provider API keys in-memory. In-memory only; these keys never
    /// touch disk. Settings snapshots project only non-secret key-presence
    /// booleans.
    SetProviderApiKeys {
        open_router: Option<String>,
        ollama: Option<String>,
        eleven_labs: Option<String>,
        assembly_ai: Option<String>,
        perplexity: Option<String>,
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
        &self,
        action: Self::Action,
        correlation_id: &str,
        send: &dyn Fn(ActorCommand),
    ) -> Result<(), String> {
        // Relay edits drive kernel-owned relay state (the `AppRelaySlot`), not
        // a `PodcastStore` write, so they emit a real relay `ActorCommand`
        // (`AddRelay`/`RemoveRelay`) instead of relying on the host-op handler
        // to mutate state. This mirrors the `podcast.discover_nostr` →
        // `EnsureInterest` precedent in `podcast_module.rs`: emitting an
        // `ActorCommand` needs the `send` closure, which only `execute` carries.
        // `add_relay` upserts on URL, so `SetRelayRole` is an `AddRelay` with
        // the new role.
        //
        // REACTIVITY: the kernel mutating `configured_relays` does NOT bump
        // `handle.rev`, and the snapshot push frame is rev-gated (an unchanged
        // rev returns the cached JSON, and iOS dedupes ticks on `rev`). So a
        // relay-only `AddRelay`/`RemoveRelay` would mutate the slot but never
        // reach the UI until some unrelated rev bump. To make the edit
        // reactive we ALSO send a `DispatchHostOp` for the same action; its
        // handler arm bumps `handle.rev`, forcing a fresh rebuild that reads
        // the just-mutated slot. The kernel actor processes commands FIFO, so
        // the slot mutation always lands before the rev-bump rebuild.
        match action {
            SettingsAction::AddRelay { ref url, ref role }
            | SettingsAction::SetRelayRole { ref url, ref role } => {
                send(ActorCommand::AddRelay {
                    url: url.clone(),
                    role: role.clone(),
                });
            }
            SettingsAction::RemoveRelay { ref url } => {
                send(ActorCommand::RemoveRelay { url: url.clone() });
            }
            _ => {}
        }

        crate::ffi::actions::dispatch_host_op(Self::NAMESPACE, &action, correlation_id, send)
    }
}

#[cfg(test)]
#[path = "settings_module_tests.rs"]
mod tests;
