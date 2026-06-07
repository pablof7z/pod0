//! Handler for `podcast.settings.*` actions.
//!
//! Most arms write a scalar onto `PodcastStore` (and occasionally the
//! `PlayerActor`) then bump `rev` so the rev-gated snapshot push frame
//! rebuilds. Relay edits are the exception — they mutate kernel-owned slot
//! state and only persist + bump here (see the arm comment).

use crate::ad_skip_handler::handle_set_auto_skip_ads;
use crate::ffi::actions::settings_module::SettingsAction;
use crate::host_op_handler::PodcastHostOpHandler;

impl PodcastHostOpHandler {
    pub(crate) fn handle_settings_action(&self, action: SettingsAction) -> serde_json::Value {
        match action {
            SettingsAction::SetAutoSkipAds { enabled } => {
                handle_set_auto_skip_ads(&self.store, &self.player_actor, &self.rev, enabled)
            }
            SettingsAction::SetSkipIntervals {
                forward_secs,
                backward_secs,
            } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_skip_intervals(forward_secs, backward_secs);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetAutoPlayNext { enabled } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_auto_play_next(enabled);
                }
                if let Ok(mut a) = self.player_actor.lock() {
                    a.set_auto_play_next(enabled);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetAutoMarkPlayedAtEnd { enabled } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_auto_mark_played_at_end(enabled);
                }
                if let Ok(mut a) = self.player_actor.lock() {
                    a.set_auto_mark_played_at_end(enabled);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetHeadphoneGestureActions {
                double_tap,
                triple_tap,
            } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_headphone_gesture_actions(double_tap, triple_tap);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetDefaultPlaybackRate { rate } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_default_playback_rate(rate);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetAutoDeleteDownloadsAfterPlayed { enabled } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_auto_delete_downloads_after_played(enabled);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetAgentInitialModel { model, model_name } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_agent_initial_model(model, model_name);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetAgentThinkingModel { model, model_name } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_agent_thinking_model(model, model_name);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetMemoryCompilationModel { model, model_name } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_memory_compilation_model(model, model_name);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetWikiModel { model, model_name } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_wiki_model(model, model_name);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetCategorizationModel { model, model_name } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_categorization_model(model, model_name);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetChapterCompilationModel { model, model_name } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_chapter_compilation_model(model, model_name);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetEmbeddingsModel { model, model_name } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_embeddings_model(model, model_name);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetImageGenerationModel { model, model_name } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_image_generation_model(model, model_name);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetRerankerEnabled { enabled } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_reranker_enabled(enabled);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetOpenRouterCredential {
                source,
                key_id,
                key_label,
                connected_at,
            } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_open_router_credential(source, key_id, key_label, connected_at);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetOllamaCredential {
                source,
                key_id,
                key_label,
                connected_at,
            } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_ollama_credential(source, key_id, key_label, connected_at);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetOllamaChatUrl { url } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_ollama_chat_url(url);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetElevenLabsCredential {
                source,
                key_id,
                key_label,
                connected_at,
            } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_eleven_labs_credential(source, key_id, key_label, connected_at);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetSttProvider { provider } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_stt_provider(provider);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetSttKeysPresent { providers } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_stt_keys_present(providers);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetOpenRouterWhisperModel { model } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_open_router_whisper_model(model);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetAssemblyAiSttModel { model } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_assembly_ai_stt_model(model);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetElevenLabsModels {
                stt_model,
                tts_model,
            } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_eleven_labs_models(stt_model, tts_model);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetElevenLabsVoice {
                voice_id,
                voice_name,
            } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_eleven_labs_voice(voice_id, voice_name);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetBlossomServerUrl { url } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_blossom_server_url(url);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetYoutubeExtractorUrl { url } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_youtube_extractor_url(url);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetLocalModel { model_id } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_local_model_id(model_id);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetWikiAutoGenerateOnTranscriptIngest { enabled } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_wiki_auto_generate_on_transcript_ingest(enabled);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetAutoIngestPublisherTranscripts { enabled } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_auto_ingest_publisher_transcripts(enabled);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetAutoFallbackToScribe { enabled } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_auto_fallback_to_scribe(enabled);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetNotifyOnNewEpisodes { enabled } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_notify_on_new_episodes(enabled);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetNostrEnabled { enabled } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_nostr_enabled(enabled);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetNostrRelayUrl { url } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_nostr_relay_url(url);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetNostrPublicRelays { relays } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_nostr_public_relays(relays);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetNostrProfile {
                name,
                about,
                picture,
            } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_nostr_profile(name, about, picture);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetNostrPublicKeyHex { hex } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_nostr_public_key_hex(hex);
                }
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            SettingsAction::SetProviderApiKeys {
                open_router,
                ollama,
                eleven_labs,
                assembly_ai,
                perplexity,
            } => {
                if let Ok(mut s) = self.store.lock() {
                    s.set_provider_api_keys(
                        open_router,
                        ollama,
                        eleven_labs,
                        assembly_ai,
                        perplexity,
                    );
                }
                serde_json::json!({"ok": true})
            }
            // Relay edits mutate kernel-owned state (the `AppRelaySlot`), not
            // `PodcastStore` — `SettingsActionModule::execute` already emitted
            // the real `ActorCommand::AddRelay` / `RemoveRelay` that mutates the
            // slot. This `DispatchHostOp` companion does two things:
            //
            //   1. Bumps `handle.rev` so the rev-gated snapshot push frame
            //      rebuilds and the new relay list reaches iOS (a relay-only
            //      ActorCommand never bumps rev).
            //   2. Persists the full post-mutation relay list to the
            //      `.nmp-relay-config.json` sidecar so the edit survives an app
            //      restart on the raw C-ABI start path (the builder sidecar is
            //      unreachable here — see `store::relay_config`).
            //
            // FIFO actor ordering guarantees the slot mutation landed before
            // this arm runs, so reading `configured_relays_handle()` here sees
            // the just-applied edit — the kernel slot is the source of truth,
            // identical to what the snapshot projection reads.
            SettingsAction::AddRelay { .. }
            | SettingsAction::RemoveRelay { .. }
            | SettingsAction::SetRelayRole { .. } => {
                crate::ffi::relay_persist::persist_configured_relays(self.app, &self.store);
                self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
        }
    }
}
