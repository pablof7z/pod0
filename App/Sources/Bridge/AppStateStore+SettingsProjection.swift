import Foundation

extension AppStateStore {
    /// Project snapshot-derived settings + last-played episode onto a working
    /// `AppState` copy. Shared by the full projection and snapshot-only fast
    /// path so provider/model render state cannot drift between code paths.
    func projectSnapshotDerivedState(into next: inout AppState, snapshot: PodcastUpdate?) {
        if let ks = snapshot?.settings {
            projectSettingsSnapshot(ks, into: &next)
        }

        if let episodeIdStr = snapshot?.nowPlaying?.episodeId,
           let uuid = UUID(uuidString: episodeIdStr) {
            next.lastPlayedEpisodeID = uuid
        }

        // ── nostrConversations: wire DTO → domain record ──────────────────────
        // The kernel is AUTHORITATIVE for the conversation projection — when the
        // social domain delivers a non-empty slice, it REPLACES the local slice.
        // An empty projection (no conversations in the kernel) leaves the
        // Swift-local slice untouched so locally-recorded outgoing turns remain
        // visible until the next authoritative kernel push.
        if let dtos = snapshot?.nostrConversations, !dtos.isEmpty {
            next.nostrConversations = dtos.map { KernelModel.nostrConversationFromDTO($0) }
        }
    }

    private func projectSettingsSnapshot(_ ks: SettingsSnapshot, into next: inout AppState) {
        // Preserve Swift-persisted `true` until Rust learns about it via the
        // `update_settings` dispatch that fires on the same change.
        next.settings.hasCompletedOnboarding = ks.hasCompletedOnboarding || state.settings.hasCompletedOnboarding
        next.settings.autoSkipAds = ks.autoSkipAdsEnabled
        next.settings.autoPlayNext = ks.autoPlayNext
        next.settings.autoMarkPlayedAtEnd = ks.autoMarkPlayedAtEnd

        if let doubleTap = HeadphoneGestureAction(rawValue: ks.headphoneDoubleTapAction) {
            next.settings.headphoneDoubleTapAction = doubleTap
        }
        if let tripleTap = HeadphoneGestureAction(rawValue: ks.headphoneTripleTapAction) {
            next.settings.headphoneTripleTapAction = tripleTap
        }

        next.settings.skipForwardSeconds = Int(ks.skipForwardSecs)
        next.settings.skipBackwardSeconds = Int(ks.skipBackwardSecs)
        // #561: project kernel-persisted playback rate so `store.state.settings.defaultPlaybackRate`
        // reflects the value from podcasts.json on cold relaunch. Without this projection,
        // `onChange(of: store.state.settings)` never fires with a non-zero delta and the
        // engine rate stays at 1.0× even after the kernel has restored 1.5× from disk.
        next.settings.defaultPlaybackRate = ks.defaultPlaybackRate
        next.settings.agentInitialModel = ks.agentInitialModel
        next.settings.agentInitialModelName = ks.agentInitialModelName
        next.settings.agentThinkingModel = ks.agentThinkingModel
        next.settings.agentThinkingModelName = ks.agentThinkingModelName
        next.settings.memoryCompilationModel = ks.memoryCompilationModel
        next.settings.memoryCompilationModelName = ks.memoryCompilationModelName
        next.settings.categorizationModel = ks.categorizationModel
        next.settings.categorizationModelName = ks.categorizationModelName
        next.settings.chapterCompilationModel = ks.chapterCompilationModel
        next.settings.chapterCompilationModelName = ks.chapterCompilationModelName
        next.settings.embeddingsModel = ks.embeddingsModel
        next.settings.embeddingsModelName = ks.embeddingsModelName
        next.settings.imageGenerationModel = ks.imageGenerationModel
        next.settings.imageGenerationModelName = ks.imageGenerationModelName
        next.settings.rerankerEnabled = ks.rerankerEnabled
        next.settings.openRouterCredentialSource = ks.openRouterSource
        next.settings.openRouterBYOKKeyID = ks.openRouterBYOKKeyID
        next.settings.openRouterBYOKKeyLabel = ks.openRouterBYOKKeyLabel
        next.settings.openRouterConnectedAt = ks.openRouterConnectedAt
        next.settings.ollamaCredentialSource = ks.ollamaSource
        next.settings.ollamaBYOKKeyID = ks.ollamaBYOKKeyID
        next.settings.ollamaBYOKKeyLabel = ks.ollamaBYOKKeyLabel
        next.settings.ollamaConnectedAt = ks.ollamaConnectedAt
        next.settings.ollamaChatURL = ks.ollamaChatURL
        next.settings.elevenLabsCredentialSource = ks.elevenLabsSource
        next.settings.elevenLabsBYOKKeyID = ks.elevenLabsBYOKKeyID
        next.settings.elevenLabsBYOKKeyLabel = ks.elevenLabsBYOKKeyLabel
        next.settings.elevenLabsConnectedAt = ks.elevenLabsConnectedAt
        next.settings.assemblyAICredentialSource = ks.assemblyAISource
        next.settings.assemblyAIBYOKKeyID = ks.assemblyAIBYOKKeyID
        next.settings.assemblyAIBYOKKeyLabel = ks.assemblyAIBYOKKeyLabel
        next.settings.assemblyAIConnectedAt = ks.assemblyAIConnectedAt
        next.settings.perplexityCredentialSource = ks.perplexitySource
        next.settings.perplexityBYOKKeyID = ks.perplexityBYOKKeyID
        next.settings.perplexityBYOKKeyLabel = ks.perplexityBYOKKeyLabel
        next.settings.perplexityConnectedAt = ks.perplexityConnectedAt

        if let sttProvider = STTProvider(rawValue: ks.sttProvider) {
            next.settings.sttProvider = sttProvider
        }
        next.settings.openRouterWhisperModel = ks.openRouterWhisperModel
        next.settings.assemblyAISTTModel = ks.assemblyAISTTModel
        next.settings.elevenLabsSTTModel = ks.elevenLabsSTTModel
        next.settings.elevenLabsTTSModel = ks.elevenLabsTTSModel
        next.settings.elevenLabsVoiceID = ks.elevenLabsVoiceID
        next.settings.elevenLabsVoiceName = ks.elevenLabsVoiceName
        next.settings.localModelID = ks.localModelID

        // #561 seam: fields that were dispatched Swift→kernel but never projected
        // back, causing stale values on cold relaunch / cross-update.
        next.settings.blossomServerURL = ks.blossomServerURL
        next.settings.youtubeExtractorURL = ks.youtubeExtractorURL
        next.settings.autoDeleteDownloadsAfterPlayed = ks.autoDeleteDownloadsAfterPlayed
        next.settings.autoIngestPublisherTranscripts = ks.autoIngestPublisherTranscripts
        next.settings.autoFallbackToScribe = ks.autoFallbackToScribe
        next.settings.notifyOnNewEpisodes = ks.notifyOnNewEpisodes
        next.settings.nostrEnabled = ks.nostrEnabled
        next.settings.nostrRelayURL = ks.nostrRelayURL
        next.settings.nostrProfileName = ks.nostrProfileName
        next.settings.nostrProfileAbout = ks.nostrProfileAbout
        next.settings.nostrProfilePicture = ks.nostrProfilePicture
        // nostrPublicKeyHex is kernel-derived (from Keychain via Rust); Swift
        // never dispatches it — project the kernel's computed value back so
        // store.state.settings.nostrPublicKeyHex stays current.
        next.settings.nostrPublicKeyHex = ks.nostrPublicKeyHex
    }
}
