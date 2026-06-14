import Foundation

// MARK: - AppStateStore + Settings

extension AppStateStore {

    /// A fully-composed `AppState` snapshot with the live `episodes` folded
    /// back into the DTO. Use this for the handful of consumers that take a
    /// whole `AppState` and need `episodes` populated (data-export stats,
    /// local search). Prefer reading `store.episodes` directly in view bodies —
    /// this allocates a struct copy, so it is *not* a per-cell read path.
    var composedState: AppState {
        var snapshot = state
        snapshot.episodes = episodes
        return snapshot
    }

    func updateSettings(_ settings: Settings) {
        // Echo suppression: if the iCloud capability just applied a remote
        // change via dispatchSilent, skip the outbound dispatch path so we
        // do not re-echo the value back to the cloud. The kernel snapshot
        // update (from dispatchSilent) will trigger this method; we suppress
        // it here and let the capability's applySettingsSnapshot handle
        // the outbound side.
        guard !isApplyingRemoteChange else {
            state.settings = settings
            return
        }
        let prior = state.settings
        state.settings = settings
        // Mirror the Rust-owned subset of settings to the kernel so they
        // survive across restarts (Rust persists them in podcasts.json).
        if settings.autoSkipAds != prior.autoSkipAds {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_auto_skip_ads", "enabled": settings.autoSkipAds])
        }
        if settings.skipForwardSeconds != prior.skipForwardSeconds
            || settings.skipBackwardSeconds != prior.skipBackwardSeconds {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_skip_intervals",
                                 "forward_secs": Double(settings.skipForwardSeconds),
                                 "backward_secs": Double(settings.skipBackwardSeconds)
                             ])
        }
        if settings.autoPlayNext != prior.autoPlayNext {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_auto_play_next", "enabled": settings.autoPlayNext])
        }
        if settings.autoMarkPlayedAtEnd != prior.autoMarkPlayedAtEnd {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_auto_mark_played_at_end", "enabled": settings.autoMarkPlayedAtEnd])
        }
        if settings.headphoneDoubleTapAction != prior.headphoneDoubleTapAction
            || settings.headphoneTripleTapAction != prior.headphoneTripleTapAction {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_headphone_gesture_actions",
                                 "double_tap": settings.headphoneDoubleTapAction.rawValue,
                                 "triple_tap": settings.headphoneTripleTapAction.rawValue
                             ])
        }
        if settings.hasCompletedOnboarding != prior.hasCompletedOnboarding {
            kernel?.dispatch(namespace: "podcast",
                             body: [
                                 "op": "update_settings",
                                 "has_completed_onboarding": settings.hasCompletedOnboarding
                             ])
        }
        if settings.defaultPlaybackRate != prior.defaultPlaybackRate {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_default_playback_rate", "rate": settings.defaultPlaybackRate])
        }
        if settings.autoDeleteDownloadsAfterPlayed != prior.autoDeleteDownloadsAfterPlayed {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_auto_delete_downloads_after_played",
                                    "enabled": settings.autoDeleteDownloadsAfterPlayed])
        }
        if settings.agentInitialModel != prior.agentInitialModel
            || settings.agentInitialModelName != prior.agentInitialModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_agent_initial_model",
                                 "model": settings.agentInitialModel,
                                 "model_name": settings.agentInitialModelName
                             ])
        }
        if settings.agentThinkingModel != prior.agentThinkingModel
            || settings.agentThinkingModelName != prior.agentThinkingModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_agent_thinking_model",
                                 "model": settings.agentThinkingModel,
                                 "model_name": settings.agentThinkingModelName
                             ])
        }
        if settings.memoryCompilationModel != prior.memoryCompilationModel
            || settings.memoryCompilationModelName != prior.memoryCompilationModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_memory_compilation_model",
                                 "model": settings.memoryCompilationModel,
                                 "model_name": settings.memoryCompilationModelName
                             ])
        }
        if settings.wikiModel != prior.wikiModel
            || settings.wikiModelName != prior.wikiModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_wiki_model",
                                 "model": settings.wikiModel,
                                 "model_name": settings.wikiModelName
                             ])
        }
        if settings.categorizationModel != prior.categorizationModel
            || settings.categorizationModelName != prior.categorizationModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_categorization_model",
                                 "model": settings.categorizationModel,
                                 "model_name": settings.categorizationModelName
                             ])
        }
        if settings.chapterCompilationModel != prior.chapterCompilationModel
            || settings.chapterCompilationModelName != prior.chapterCompilationModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_chapter_compilation_model",
                                 "model": settings.chapterCompilationModel,
                                 "model_name": settings.chapterCompilationModelName
                             ])
        }
        if settings.embeddingsModel != prior.embeddingsModel
            || settings.embeddingsModelName != prior.embeddingsModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_embeddings_model",
                                 "model": settings.embeddingsModel,
                                 "model_name": settings.embeddingsModelName
                             ])
        }
        if settings.imageGenerationModel != prior.imageGenerationModel
            || settings.imageGenerationModelName != prior.imageGenerationModelName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_image_generation_model",
                                 "model": settings.imageGenerationModel,
                                 "model_name": settings.imageGenerationModelName
                             ])
        }
        if settings.rerankerEnabled != prior.rerankerEnabled {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_reranker_enabled", "enabled": settings.rerankerEnabled])
        }
        if settings.openRouterCredentialSource != prior.openRouterCredentialSource
            || settings.openRouterBYOKKeyID != prior.openRouterBYOKKeyID
            || settings.openRouterBYOKKeyLabel != prior.openRouterBYOKKeyLabel {
            // connected_at is stamped by the kernel (D9); omit from payload.
            dispatchCredentialMetadata(
                op: "set_open_router_credential",
                source: settings.openRouterCredentialSource.rawValue,
                keyID: settings.openRouterBYOKKeyID,
                keyLabel: settings.openRouterBYOKKeyLabel
            )
        }
        if settings.ollamaCredentialSource != prior.ollamaCredentialSource
            || settings.ollamaBYOKKeyID != prior.ollamaBYOKKeyID
            || settings.ollamaBYOKKeyLabel != prior.ollamaBYOKKeyLabel {
            dispatchCredentialMetadata(
                op: "set_ollama_credential",
                source: settings.ollamaCredentialSource.rawValue,
                keyID: settings.ollamaBYOKKeyID,
                keyLabel: settings.ollamaBYOKKeyLabel
            )
        }
        if settings.ollamaChatURL != prior.ollamaChatURL {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_ollama_chat_url", "url": settings.ollamaChatURL])
        }
        if settings.elevenLabsCredentialSource != prior.elevenLabsCredentialSource
            || settings.elevenLabsBYOKKeyID != prior.elevenLabsBYOKKeyID
            || settings.elevenLabsBYOKKeyLabel != prior.elevenLabsBYOKKeyLabel {
            dispatchCredentialMetadata(
                op: "set_eleven_labs_credential",
                source: settings.elevenLabsCredentialSource.rawValue,
                keyID: settings.elevenLabsBYOKKeyID,
                keyLabel: settings.elevenLabsBYOKKeyLabel
            )
        }
        if settings.assemblyAICredentialSource != prior.assemblyAICredentialSource
            || settings.assemblyAIBYOKKeyID != prior.assemblyAIBYOKKeyID
            || settings.assemblyAIBYOKKeyLabel != prior.assemblyAIBYOKKeyLabel {
            dispatchCredentialMetadata(
                op: "set_assembly_ai_credential",
                source: settings.assemblyAICredentialSource.rawValue,
                keyID: settings.assemblyAIBYOKKeyID,
                keyLabel: settings.assemblyAIBYOKKeyLabel
            )
        }
        if settings.perplexityCredentialSource != prior.perplexityCredentialSource
            || settings.perplexityBYOKKeyID != prior.perplexityBYOKKeyID
            || settings.perplexityBYOKKeyLabel != prior.perplexityBYOKKeyLabel {
            dispatchCredentialMetadata(
                op: "set_perplexity_credential",
                source: settings.perplexityCredentialSource.rawValue,
                keyID: settings.perplexityBYOKKeyID,
                keyLabel: settings.perplexityBYOKKeyLabel
            )
        }
        if settings.sttProvider != prior.sttProvider {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_stt_provider", "provider": settings.sttProvider])
        }
        if settings.openRouterWhisperModel != prior.openRouterWhisperModel {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_open_router_whisper_model", "model": settings.openRouterWhisperModel])
        }
        if settings.assemblyAISTTModel != prior.assemblyAISTTModel {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_assembly_ai_stt_model", "model": settings.assemblyAISTTModel])
        }
        if settings.elevenLabsSTTModel != prior.elevenLabsSTTModel
            || settings.elevenLabsTTSModel != prior.elevenLabsTTSModel {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_eleven_labs_models",
                                 "stt_model": settings.elevenLabsSTTModel,
                                 "tts_model": settings.elevenLabsTTSModel
                             ])
        }
        if settings.elevenLabsVoiceID != prior.elevenLabsVoiceID
            || settings.elevenLabsVoiceName != prior.elevenLabsVoiceName {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_eleven_labs_voice",
                                 "voice_id": settings.elevenLabsVoiceID,
                                 "voice_name": settings.elevenLabsVoiceName
                             ])
        }
        if settings.blossomServerURL != prior.blossomServerURL {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_blossom_server_url", "url": settings.blossomServerURL])
        }
        if settings.youtubeExtractorURL != prior.youtubeExtractorURL {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_youtube_extractor_url", "url": settings.youtubeExtractorURL as Any])
        }
        if settings.wikiAutoGenerateOnTranscriptIngest != prior.wikiAutoGenerateOnTranscriptIngest {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_wiki_auto_generate_on_transcript_ingest", "enabled": settings.wikiAutoGenerateOnTranscriptIngest])
        }
        if settings.autoIngestPublisherTranscripts != prior.autoIngestPublisherTranscripts {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_auto_ingest_publisher_transcripts", "enabled": settings.autoIngestPublisherTranscripts])
        }
        if settings.autoFallbackToScribe != prior.autoFallbackToScribe {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_auto_fallback_to_scribe", "enabled": settings.autoFallbackToScribe])
        }
        if settings.notifyOnNewEpisodes != prior.notifyOnNewEpisodes {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_notify_on_new_episodes", "enabled": settings.notifyOnNewEpisodes])
        }
        if settings.nostrEnabled != prior.nostrEnabled {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_nostr_enabled", "enabled": settings.nostrEnabled])
        }
        if settings.nostrRelayURL != prior.nostrRelayURL {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_nostr_relay_url", "url": settings.nostrRelayURL])
        }
        // Profile name/about/picture are carried in a single atomic op
        // (`SetNostrProfile { name, about, picture }`) — the kernel has no
        // per-field profile ops, so the previous three separate
        // `set_nostr_profile_{name,about,picture}` dispatches failed to
        // deserialize and never reached the store. Dispatch the whole profile
        // whenever any of the three fields changes.
        if settings.nostrProfileName != prior.nostrProfileName
            || settings.nostrProfileAbout != prior.nostrProfileAbout
            || settings.nostrProfilePicture != prior.nostrProfilePicture {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: [
                                 "op": "set_nostr_profile",
                                 "name": settings.nostrProfileName,
                                 "about": settings.nostrProfileAbout,
                                 "picture": settings.nostrProfilePicture
                             ])
        }
        // "Local" is a per-role provider now (a role's model is `local:<id>`),
        // not a global switch. Only one on-device engine loads at a time, so we
        // derive the single engine the kernel should keep loaded from whichever
        // roles point at a local model, and push it as `set_local_model`. This
        // drives engine load/unload; per-role routing itself happens in the
        // kernel's `backend_for` off each role's own model string.
        let priorLocal = Self.effectiveLocalModelID(prior)
        let nextLocal = Self.effectiveLocalModelID(settings)
        if nextLocal != priorLocal {
            kernel?.dispatch(namespace: "podcast.settings",
                             body: ["op": "set_local_model", "model_id": nextLocal as Any])
            // Load/unload the on-device engine to match the new selection so the
            // kernel's LocalModelBackend callback actually has an engine to run.
            syncLocalEngine(for: settings)
        }
    }

    private func dispatchCredentialMetadata(
        op: String,
        source: String,
        keyID: String?,
        keyLabel: String?
        // connected_at intentionally omitted: kernel stamps time (D9).
    ) {
        kernel?.dispatch(namespace: "podcast.settings",
                         body: [
                             "op": op,
                             "source": source,
                             "key_id": keyID.map { $0 as Any } ?? NSNull(),
                             "key_label": keyLabel.map { $0 as Any } ?? NSNull()
                         ])
    }

    /// The single on-device model id the kernel should keep loaded, derived
    /// from the role assignments. Returns the first role pointing at a `local:`
    /// model (Agent Initial takes precedence), or nil when no role uses one.
    ///
    /// Only roles with a wired on-device completion call site in the kernel are
    /// considered — otherwise a `local:` selection would load an engine no role
    /// can invoke (wasted RAM), or let a non-routable role's precedence starve a
    /// routable one. Memory Compilation and Embeddings have no `backend_for`
    /// call site yet, so a `local:` selection there is a no-op (stays on cloud)
    /// until those paths are threaded.
    static func effectiveLocalModelID(_ s: Settings) -> String? {
        let roleModels = [
            s.agentInitialModel,
            s.agentThinkingModel,
            s.wikiModel,
            s.categorizationModel,
            s.chapterCompilationModel,
        ]
        for stored in roleModels {
            let ref = LLMModelReference(storedID: stored)
            if ref.provider == .local, !ref.isEmpty { return ref.modelID }
        }
        return nil
    }
}
