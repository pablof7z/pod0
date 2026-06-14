import Foundation
import os.log

// MARK: - iCloudSyncCapability — `pcst.icloud_sync.capability`
//
// iOS half of the iCloud-settings-sync surface (feature #52). Mirrors a
// curated subset of `podcast.settings` into `NSUbiquitousKeyValueStore` so
// playback speed, skip intervals, and the ad-skip toggle roam across the
// user's devices and survive a reinstall.
//
// PASSIVE SHAPE — like `PlatformCapability` / `SpotlightCapability` there
// is no request/response capability socket here. The capability is driven
// by (a) `NSUbiquitousKeyValueStoreDidChangeExternallyNotification` for
// inbound pulls and (b) the reactive `KernelModel.applyPodcastUpdate`
// path (which calls `applySettingsSnapshot` on every accepted frame) for
// outbound writes. It is therefore **not** routed through
// `PodcastCapabilities.handleJSON(_:)`.
//
// Doctrine:
//   D6 — failures never throw. A missing entitlement, an unreachable iCloud
//        account, or a kernel-action rejection all degrade silently. The
//        inbound path uses `KernelModel.dispatchSilent` so a not-yet-wired
//        `podcast.settings.*` action does not surface as a user-visible
//        toast.
//   D7 — iOS executes; Rust decides. The cloud value is dispatched as an
//        action; the kernel may clamp, reject, or transform it before
//        emitting the next snapshot. We then read the next snapshot and
//        only write back if the kernel's authoritative value differs from
//        cloud (the `lastWritten` diff cache in `applySettingsSnapshot`).

/// `NSUbiquitousKeyValueStore` mirror for the podcast app's portable
/// settings. Single instance, owned by `PodcastCapabilities`. A weak
/// reference to `KernelModel` is the dispatch surface for inbound changes;
/// the model holds this capability strongly so the weak handle stays valid
/// for the lifetime of the app.
@MainActor
final class iCloudSyncCapability {

    // MARK: - Constants

    /// Capability namespace. Reserved for future request/response wiring;
    /// today this capability is purely tick-driven.
    static let namespace = "pcst.icloud_sync.capability"

    /// Key namespace for `NSUbiquitousKeyValueStore`. Kept short — the
    /// store has a 1024-byte per-key budget and the four scalar values fit
    /// comfortably with these names.
    enum Key {
        static let speed             = "pcst.speed"
        static let skipForwardSecs   = "pcst.skip_forward_secs"
        static let skipBackwardSecs  = "pcst.skip_backward_secs"
        static let autoSkipAds       = "pcst.auto_skip_ads"
        static let autoDeleteDownloadsAfterPlayed = "pcst.auto_delete_downloads_after_played"
        static let agentInitialModel = "pcst.agent_initial_model"
        static let agentInitialModelName = "pcst.agent_initial_model_name"
        static let agentThinkingModel = "pcst.agent_thinking_model"
        static let agentThinkingModelName = "pcst.agent_thinking_model_name"
        static let memoryCompilationModel = "pcst.memory_compilation_model"
        static let memoryCompilationModelName = "pcst.memory_compilation_model_name"
        static let wikiModel = "pcst.wiki_model"
        static let wikiModelName = "pcst.wiki_model_name"
        static let categorizationModel = "pcst.categorization_model"
        static let categorizationModelName = "pcst.categorization_model_name"
        static let chapterCompilationModel = "pcst.chapter_compilation_model"
        static let chapterCompilationModelName = "pcst.chapter_compilation_model_name"
        static let embeddingsModel = "pcst.embeddings_model"
        static let embeddingsModelName = "pcst.embeddings_model_name"
        static let imageGenerationModel = "pcst.image_generation_model"
        static let imageGenerationModelName = "pcst.image_generation_model_name"
        static let rerankerEnabled = "pcst.reranker_enabled"
        static let ollamaChatUrl = "pcst.ollama_chat_url"
        static let sttProvider = "pcst.stt_provider"
        static let openRouterWhisperModel = "pcst.open_router_whisper_model"
        static let assemblyAiSttModel = "pcst.assembly_ai_stt_model"
        static let elevenLabsSttModel = "pcst.eleven_labs_stt_model"
        static let elevenLabsTtsModel = "pcst.eleven_labs_tts_model"
        static let elevenLabsVoiceId = "pcst.eleven_labs_voice_id"
        static let elevenLabsVoiceName = "pcst.eleven_labs_voice_name"
        static let blossomServerUrl = "pcst.blossom_server_url"
        static let youtubeExtractorUrl = "pcst.youtube_extractor_url"
        static let autoMarkPlayedAtEnd = "pcst.auto_mark_played_at_end"
        static let autoPlayNext = "pcst.auto_play_next"
        static let headphoneDoubleTapAction = "pcst.headphone_double_tap_action"
        static let headphoneTripleTapAction = "pcst.headphone_triple_tap_action"
        static let wikiAutoGenerateOnTranscriptIngest = "pcst.wiki_auto_generate_on_transcript_ingest"
        static let autoIngestPublisherTranscripts = "pcst.auto_ingest_publisher_transcripts"
        static let autoFallbackToScribe = "pcst.auto_fallback_to_scribe"
        static let notifyOnNewEpisodes = "pcst.notify_on_new_episodes"
        static let nostrRelayUrl = "pcst.nostr_relay_url"
        static let nostrProfileName = "pcst.nostr_profile_name"
        static let nostrProfileAbout = "pcst.nostr_profile_about"
        static let nostrProfilePicture = "pcst.nostr_profile_picture"

        /// Every key this capability owns.
        static let all: Set<String> = [
            speed, skipForwardSecs, skipBackwardSecs, autoSkipAds,
            autoDeleteDownloadsAfterPlayed,
            agentInitialModel, agentInitialModelName, agentThinkingModel, agentThinkingModelName,
            memoryCompilationModel, memoryCompilationModelName, wikiModel, wikiModelName,
            categorizationModel, categorizationModelName, chapterCompilationModel, chapterCompilationModelName,
            embeddingsModel, embeddingsModelName, imageGenerationModel, imageGenerationModelName,
            rerankerEnabled, ollamaChatUrl, sttProvider, openRouterWhisperModel,
            assemblyAiSttModel, elevenLabsSttModel, elevenLabsTtsModel, elevenLabsVoiceId, elevenLabsVoiceName,
            blossomServerUrl, youtubeExtractorUrl, autoMarkPlayedAtEnd, autoPlayNext,
            headphoneDoubleTapAction, headphoneTripleTapAction, wikiAutoGenerateOnTranscriptIngest,
            autoIngestPublisherTranscripts, autoFallbackToScribe, notifyOnNewEpisodes,
            nostrRelayUrl, nostrProfileName, nostrProfileAbout, nostrProfilePicture,
        ]
    }

    // MARK: - State

    private static let logger = Logger(subsystem: "io.f7z.podcast", category: "iCloudSync")

    private let kvs: NSUbiquitousKeyValueStore

    /// Dispatch sink. Weak so the capability does not extend the model's
    /// lifetime — the model owns the capability, not the other way round.
    weak var kernel: KernelModel?

    /// Reference to the app state store for echo suppression. Weak so the
    /// capability does not extend the store's lifetime.
    weak var appStore: AppStateStore?

    /// Retained observer token. Cleared in `stop()` so the notification
    /// centre does not call back into a stopped capability.
    private var changeObserver: NSObjectProtocol?

    /// Snapshot of the last value we wrote to (or read from) iCloud for
    /// each key. Used by `applySettingsSnapshot(_:)` to detect deltas so
    /// we only write when the value actually changed.
    private(set) var lastWritten: [String: AnyHashable] = [:]

    /// Echo-suppression flag. Set while applying an inbound iCloud change
    /// so the outbound writer does not immediately re-emit the same value.
    private(set) var isApplyingRemoteChange: Bool = false

    private var started: Bool = false

    // MARK: - Init

    init(kvs: NSUbiquitousKeyValueStore = .default) {
        self.kvs = kvs
    }

    // MARK: - Lifecycle

    /// Register for change notifications, kick off an initial KVS pull,
    /// and dispatch any non-default cloud values into the kernel. Safe to
    /// call multiple times — subsequent calls are no-ops.
    func start(kernel: KernelModel) {
        guard !started else { return }
        started = true
        self.kernel = kernel

        // Subscribe before the synchronize() — there is no race window in
        // which an external change could fire before we are listening.
        // Extract the changed-keys list inside the (non-Sendable)
        // notification closure and hop just the `[String]` across the
        // `@MainActor` boundary so Swift 6 doesn't flag the notification
        // capture as a data race.
        changeObserver = NotificationCenter.default.addObserver(
            forName: NSUbiquitousKeyValueStore.didChangeExternallyNotification,
            object: kvs,
            queue: .main
        ) { [weak self] notification in
            let changed = notification.userInfo?[NSUbiquitousKeyValueStoreChangedKeysKey]
                as? [String] ?? []
            MainActor.assumeIsolated {
                self?.handleExternalChange(changedKeys: changed)
            }
        }

        kvs.synchronize()
        dispatchKeysFromCloud(Array(Key.all))
        Self.logger.info("iCloudSyncCapability started")
    }

    /// Idempotent. Tears down the change observer and clears the dispatch
    /// handle. Does **not** clear `NSUbiquitousKeyValueStore` — those
    /// values live in the user's iCloud account.
    func stop() {
        started = false
        if let observer = changeObserver {
            NotificationCenter.default.removeObserver(observer)
        }
        changeObserver = nil
        kernel = nil
        lastWritten = [:]
        isApplyingRemoteChange = false
        appStore?.isApplyingRemoteChange = false
    }

    var isStarted: Bool { started }

    // MARK: - Outbound — snapshot → iCloud

    /// Compare `settings` against the last value we wrote and push any
    /// changed keys to iCloud. Called from `KernelModel.applyPodcastUpdate`
    /// on every accepted kernel frame (rev-gated), so writes ride the
    /// reactive push path rather than a timer.
    func applySettingsSnapshot(_ settings: SettingsKVSnapshot) {
        guard started else { return }
        if isApplyingRemoteChange {
            // Single-tick suppression. Seed `lastWritten` with the
            // kernel's view so the next genuine local edit *is* written.
            isApplyingRemoteChange = false
            appStore?.isApplyingRemoteChange = false
            settings.write(to: &lastWritten)
            return
        }
        if let v = settings.speed,
           lastWritten[Key.speed] != AnyHashable(v) {
            kvs.set(v, forKey: Key.speed)
            lastWritten[Key.speed] = AnyHashable(v)
        }
        if let v = settings.skipForwardSecs,
           lastWritten[Key.skipForwardSecs] != AnyHashable(v) {
            kvs.set(Int64(v), forKey: Key.skipForwardSecs)
            lastWritten[Key.skipForwardSecs] = AnyHashable(v)
        }
        if let v = settings.skipBackwardSecs,
           lastWritten[Key.skipBackwardSecs] != AnyHashable(v) {
            kvs.set(Int64(v), forKey: Key.skipBackwardSecs)
            lastWritten[Key.skipBackwardSecs] = AnyHashable(v)
        }
        if let v = settings.autoSkipAds,
           lastWritten[Key.autoSkipAds] != AnyHashable(v) {
            kvs.set(v, forKey: Key.autoSkipAds)
            lastWritten[Key.autoSkipAds] = AnyHashable(v)
        }
        if let v = settings.autoDeleteDownloadsAfterPlayed,
           lastWritten[Key.autoDeleteDownloadsAfterPlayed] != AnyHashable(v) {
            kvs.set(v, forKey: Key.autoDeleteDownloadsAfterPlayed)
            lastWritten[Key.autoDeleteDownloadsAfterPlayed] = AnyHashable(v)
        }
        if let v = settings.agentInitialModel,
           lastWritten[Key.agentInitialModel] != AnyHashable(v) {
            kvs.set(v, forKey: Key.agentInitialModel)
            lastWritten[Key.agentInitialModel] = AnyHashable(v)
        }
        if let v = settings.agentInitialModelName,
           lastWritten[Key.agentInitialModelName] != AnyHashable(v) {
            kvs.set(v, forKey: Key.agentInitialModelName)
            lastWritten[Key.agentInitialModelName] = AnyHashable(v)
        }
        if let v = settings.agentThinkingModel,
           lastWritten[Key.agentThinkingModel] != AnyHashable(v) {
            kvs.set(v, forKey: Key.agentThinkingModel)
            lastWritten[Key.agentThinkingModel] = AnyHashable(v)
        }
        if let v = settings.agentThinkingModelName,
           lastWritten[Key.agentThinkingModelName] != AnyHashable(v) {
            kvs.set(v, forKey: Key.agentThinkingModelName)
            lastWritten[Key.agentThinkingModelName] = AnyHashable(v)
        }
        if let v = settings.memoryCompilationModel,
           lastWritten[Key.memoryCompilationModel] != AnyHashable(v) {
            kvs.set(v, forKey: Key.memoryCompilationModel)
            lastWritten[Key.memoryCompilationModel] = AnyHashable(v)
        }
        if let v = settings.memoryCompilationModelName,
           lastWritten[Key.memoryCompilationModelName] != AnyHashable(v) {
            kvs.set(v, forKey: Key.memoryCompilationModelName)
            lastWritten[Key.memoryCompilationModelName] = AnyHashable(v)
        }
        if let v = settings.wikiModel,
           lastWritten[Key.wikiModel] != AnyHashable(v) {
            kvs.set(v, forKey: Key.wikiModel)
            lastWritten[Key.wikiModel] = AnyHashable(v)
        }
        if let v = settings.wikiModelName,
           lastWritten[Key.wikiModelName] != AnyHashable(v) {
            kvs.set(v, forKey: Key.wikiModelName)
            lastWritten[Key.wikiModelName] = AnyHashable(v)
        }
        if let v = settings.categorizationModel,
           lastWritten[Key.categorizationModel] != AnyHashable(v) {
            kvs.set(v, forKey: Key.categorizationModel)
            lastWritten[Key.categorizationModel] = AnyHashable(v)
        }
        if let v = settings.categorizationModelName,
           lastWritten[Key.categorizationModelName] != AnyHashable(v) {
            kvs.set(v, forKey: Key.categorizationModelName)
            lastWritten[Key.categorizationModelName] = AnyHashable(v)
        }
        if let v = settings.chapterCompilationModel,
           lastWritten[Key.chapterCompilationModel] != AnyHashable(v) {
            kvs.set(v, forKey: Key.chapterCompilationModel)
            lastWritten[Key.chapterCompilationModel] = AnyHashable(v)
        }
        if let v = settings.chapterCompilationModelName,
           lastWritten[Key.chapterCompilationModelName] != AnyHashable(v) {
            kvs.set(v, forKey: Key.chapterCompilationModelName)
            lastWritten[Key.chapterCompilationModelName] = AnyHashable(v)
        }
        if let v = settings.embeddingsModel,
           lastWritten[Key.embeddingsModel] != AnyHashable(v) {
            kvs.set(v, forKey: Key.embeddingsModel)
            lastWritten[Key.embeddingsModel] = AnyHashable(v)
        }
        if let v = settings.embeddingsModelName,
           lastWritten[Key.embeddingsModelName] != AnyHashable(v) {
            kvs.set(v, forKey: Key.embeddingsModelName)
            lastWritten[Key.embeddingsModelName] = AnyHashable(v)
        }
        if let v = settings.imageGenerationModel,
           lastWritten[Key.imageGenerationModel] != AnyHashable(v) {
            kvs.set(v, forKey: Key.imageGenerationModel)
            lastWritten[Key.imageGenerationModel] = AnyHashable(v)
        }
        if let v = settings.imageGenerationModelName,
           lastWritten[Key.imageGenerationModelName] != AnyHashable(v) {
            kvs.set(v, forKey: Key.imageGenerationModelName)
            lastWritten[Key.imageGenerationModelName] = AnyHashable(v)
        }
        if let v = settings.rerankerEnabled,
           lastWritten[Key.rerankerEnabled] != AnyHashable(v) {
            kvs.set(v, forKey: Key.rerankerEnabled)
            lastWritten[Key.rerankerEnabled] = AnyHashable(v)
        }
        if let v = settings.ollamaChatUrl,
           lastWritten[Key.ollamaChatUrl] != AnyHashable(v) {
            kvs.set(v, forKey: Key.ollamaChatUrl)
            lastWritten[Key.ollamaChatUrl] = AnyHashable(v)
        }
        if let v = settings.sttProvider,
           lastWritten[Key.sttProvider] != AnyHashable(v) {
            kvs.set(v, forKey: Key.sttProvider)
            lastWritten[Key.sttProvider] = AnyHashable(v)
        }
        if let v = settings.openRouterWhisperModel,
           lastWritten[Key.openRouterWhisperModel] != AnyHashable(v) {
            kvs.set(v, forKey: Key.openRouterWhisperModel)
            lastWritten[Key.openRouterWhisperModel] = AnyHashable(v)
        }
        if let v = settings.assemblyAiSttModel,
           lastWritten[Key.assemblyAiSttModel] != AnyHashable(v) {
            kvs.set(v, forKey: Key.assemblyAiSttModel)
            lastWritten[Key.assemblyAiSttModel] = AnyHashable(v)
        }
        if let v = settings.elevenLabsSttModel,
           lastWritten[Key.elevenLabsSttModel] != AnyHashable(v) {
            kvs.set(v, forKey: Key.elevenLabsSttModel)
            lastWritten[Key.elevenLabsSttModel] = AnyHashable(v)
        }
        if let v = settings.elevenLabsTtsModel,
           lastWritten[Key.elevenLabsTtsModel] != AnyHashable(v) {
            kvs.set(v, forKey: Key.elevenLabsTtsModel)
            lastWritten[Key.elevenLabsTtsModel] = AnyHashable(v)
        }
        if let v = settings.elevenLabsVoiceId,
           lastWritten[Key.elevenLabsVoiceId] != AnyHashable(v) {
            kvs.set(v, forKey: Key.elevenLabsVoiceId)
            lastWritten[Key.elevenLabsVoiceId] = AnyHashable(v)
        }
        if let v = settings.elevenLabsVoiceName,
           lastWritten[Key.elevenLabsVoiceName] != AnyHashable(v) {
            kvs.set(v, forKey: Key.elevenLabsVoiceName)
            lastWritten[Key.elevenLabsVoiceName] = AnyHashable(v)
        }
        if let v = settings.blossomServerUrl,
           lastWritten[Key.blossomServerUrl] != AnyHashable(v) {
            kvs.set(v, forKey: Key.blossomServerUrl)
            lastWritten[Key.blossomServerUrl] = AnyHashable(v)
        }
        if let v = settings.youtubeExtractorUrl,
           lastWritten[Key.youtubeExtractorUrl] != AnyHashable(v) {
            kvs.set(v, forKey: Key.youtubeExtractorUrl)
            lastWritten[Key.youtubeExtractorUrl] = AnyHashable(v)
        }
        if let v = settings.autoMarkPlayedAtEnd,
           lastWritten[Key.autoMarkPlayedAtEnd] != AnyHashable(v) {
            kvs.set(v, forKey: Key.autoMarkPlayedAtEnd)
            lastWritten[Key.autoMarkPlayedAtEnd] = AnyHashable(v)
        }
        if let v = settings.autoPlayNext,
           lastWritten[Key.autoPlayNext] != AnyHashable(v) {
            kvs.set(v, forKey: Key.autoPlayNext)
            lastWritten[Key.autoPlayNext] = AnyHashable(v)
        }
        if let v = settings.headphoneDoubleTapAction,
           lastWritten[Key.headphoneDoubleTapAction] != AnyHashable(v) {
            kvs.set(v, forKey: Key.headphoneDoubleTapAction)
            lastWritten[Key.headphoneDoubleTapAction] = AnyHashable(v)
        }
        if let v = settings.headphoneTripleTapAction,
           lastWritten[Key.headphoneTripleTapAction] != AnyHashable(v) {
            kvs.set(v, forKey: Key.headphoneTripleTapAction)
            lastWritten[Key.headphoneTripleTapAction] = AnyHashable(v)
        }
        if let v = settings.wikiAutoGenerateOnTranscriptIngest,
           lastWritten[Key.wikiAutoGenerateOnTranscriptIngest] != AnyHashable(v) {
            kvs.set(v, forKey: Key.wikiAutoGenerateOnTranscriptIngest)
            lastWritten[Key.wikiAutoGenerateOnTranscriptIngest] = AnyHashable(v)
        }
        if let v = settings.autoIngestPublisherTranscripts,
           lastWritten[Key.autoIngestPublisherTranscripts] != AnyHashable(v) {
            kvs.set(v, forKey: Key.autoIngestPublisherTranscripts)
            lastWritten[Key.autoIngestPublisherTranscripts] = AnyHashable(v)
        }
        if let v = settings.autoFallbackToScribe,
           lastWritten[Key.autoFallbackToScribe] != AnyHashable(v) {
            kvs.set(v, forKey: Key.autoFallbackToScribe)
            lastWritten[Key.autoFallbackToScribe] = AnyHashable(v)
        }
        if let v = settings.notifyOnNewEpisodes,
           lastWritten[Key.notifyOnNewEpisodes] != AnyHashable(v) {
            kvs.set(v, forKey: Key.notifyOnNewEpisodes)
            lastWritten[Key.notifyOnNewEpisodes] = AnyHashable(v)
        }
        if let v = settings.nostrRelayUrl,
           lastWritten[Key.nostrRelayUrl] != AnyHashable(v) {
            kvs.set(v, forKey: Key.nostrRelayUrl)
            lastWritten[Key.nostrRelayUrl] = AnyHashable(v)
        }
        if let v = settings.nostrProfileName,
           lastWritten[Key.nostrProfileName] != AnyHashable(v) {
            kvs.set(v, forKey: Key.nostrProfileName)
            lastWritten[Key.nostrProfileName] = AnyHashable(v)
        }
        if let v = settings.nostrProfileAbout,
           lastWritten[Key.nostrProfileAbout] != AnyHashable(v) {
            kvs.set(v, forKey: Key.nostrProfileAbout)
            lastWritten[Key.nostrProfileAbout] = AnyHashable(v)
        }
        if let v = settings.nostrProfilePicture,
           lastWritten[Key.nostrProfilePicture] != AnyHashable(v) {
            kvs.set(v, forKey: Key.nostrProfilePicture)
            lastWritten[Key.nostrProfilePicture] = AnyHashable(v)
        }
    }

    // MARK: - Inbound — iCloud → snapshot

    /// Handle a single
    /// `NSUbiquitousKeyValueStoreDidChangeExternallyNotification`. Takes
    /// the changed-keys list (extracted on the producing side so the hop
    /// across the `@MainActor` boundary stays Sendable) and dispatches
    /// the matching `podcast.settings.*` action for each tracked key.
    ///
    /// Internal so the test suite can drive this path directly without
    /// having to fire an actual KVS notification.
    func handleExternalChange(changedKeys: [String]) {
        let tracked = changedKeys.filter { Key.all.contains($0) }
        guard !tracked.isEmpty else { return }
        Self.logger.info("KVS external change: \(tracked.joined(separator: ","), privacy: .public)")
        dispatchKeysFromCloud(tracked)
    }

    /// Shared dispatch path used by both the on-launch merge and the
    /// external-change observer. Skip-interval requires both values
    /// together (the action takes `forward` + `backward`) so the two
    /// keys are coalesced into a single dispatch.
    ///
    /// Dispatched via `KernelModel.dispatchSilent` — a rejection from a
    /// not-yet-wired Rust action should not surface as a user toast.
    private func dispatchKeysFromCloud(_ keys: [String]) {
        let touched = Set(keys)
        var didDispatch = false
        // Set the echo-suppression flag on the app store so updateSettings
        // does not re-dispatch the same values back to the kernel.
        appStore?.isApplyingRemoteChange = true

        if touched.contains(Key.skipForwardSecs) || touched.contains(Key.skipBackwardSecs),
           let forward = (kvs.object(forKey: Key.skipForwardSecs) as? NSNumber)?.intValue,
           let backward = (kvs.object(forKey: Key.skipBackwardSecs) as? NSNumber)?.intValue,
           lastWritten[Key.skipForwardSecs] != AnyHashable(forward)
             || lastWritten[Key.skipBackwardSecs] != AnyHashable(backward) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings", body: [
                "op": "set_skip_intervals",
                "forward_secs": Double(forward),
                "backward_secs": Double(backward),
            ])
            lastWritten[Key.skipForwardSecs] = AnyHashable(forward)
            lastWritten[Key.skipBackwardSecs] = AnyHashable(backward)
            didDispatch = true
        }

        if touched.contains(Key.autoSkipAds),
           let enabled = (kvs.object(forKey: Key.autoSkipAds) as? NSNumber)?.boolValue,
           lastWritten[Key.autoSkipAds] != AnyHashable(enabled) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_auto_skip_ads", "enabled": enabled])
            lastWritten[Key.autoSkipAds] = AnyHashable(enabled)
            didDispatch = true
        }

        if touched.contains(Key.speed),
           let speed = (kvs.object(forKey: Key.speed) as? NSNumber)?.doubleValue,
           lastWritten[Key.speed] != AnyHashable(speed) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_default_playback_rate", "rate": speed])
            lastWritten[Key.speed] = AnyHashable(speed)
            didDispatch = true
        }

        if touched.contains(Key.autoDeleteDownloadsAfterPlayed),
           let enabled = (kvs.object(forKey: Key.autoDeleteDownloadsAfterPlayed) as? NSNumber)?.boolValue,
           lastWritten[Key.autoDeleteDownloadsAfterPlayed] != AnyHashable(enabled) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_auto_delete_downloads_after_played", "enabled": enabled])
            lastWritten[Key.autoDeleteDownloadsAfterPlayed] = AnyHashable(enabled)
            didDispatch = true
        }

        if touched.contains(Key.agentInitialModel) || touched.contains(Key.agentInitialModelName),
           let model = (kvs.object(forKey: Key.agentInitialModel) as? String),
           let modelName = (kvs.object(forKey: Key.agentInitialModelName) as? String),
           lastWritten[Key.agentInitialModel] != AnyHashable(model)
             || lastWritten[Key.agentInitialModelName] != AnyHashable(modelName) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings", body: [
                "op": "set_agent_initial_model",
                "model": model,
                "model_name": modelName,
            ])
            lastWritten[Key.agentInitialModel] = AnyHashable(model)
            lastWritten[Key.agentInitialModelName] = AnyHashable(modelName)
            didDispatch = true
        }

        if touched.contains(Key.agentThinkingModel) || touched.contains(Key.agentThinkingModelName),
           let model = (kvs.object(forKey: Key.agentThinkingModel) as? String),
           let modelName = (kvs.object(forKey: Key.agentThinkingModelName) as? String),
           lastWritten[Key.agentThinkingModel] != AnyHashable(model)
             || lastWritten[Key.agentThinkingModelName] != AnyHashable(modelName) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings", body: [
                "op": "set_agent_thinking_model",
                "model": model,
                "model_name": modelName,
            ])
            lastWritten[Key.agentThinkingModel] = AnyHashable(model)
            lastWritten[Key.agentThinkingModelName] = AnyHashable(modelName)
            didDispatch = true
        }

        if touched.contains(Key.memoryCompilationModel) || touched.contains(Key.memoryCompilationModelName),
           let model = (kvs.object(forKey: Key.memoryCompilationModel) as? String),
           let modelName = (kvs.object(forKey: Key.memoryCompilationModelName) as? String),
           lastWritten[Key.memoryCompilationModel] != AnyHashable(model)
             || lastWritten[Key.memoryCompilationModelName] != AnyHashable(modelName) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings", body: [
                "op": "set_memory_compilation_model",
                "model": model,
                "model_name": modelName,
            ])
            lastWritten[Key.memoryCompilationModel] = AnyHashable(model)
            lastWritten[Key.memoryCompilationModelName] = AnyHashable(modelName)
            didDispatch = true
        }

        if touched.contains(Key.wikiModel) || touched.contains(Key.wikiModelName),
           let model = (kvs.object(forKey: Key.wikiModel) as? String),
           let modelName = (kvs.object(forKey: Key.wikiModelName) as? String),
           lastWritten[Key.wikiModel] != AnyHashable(model)
             || lastWritten[Key.wikiModelName] != AnyHashable(modelName) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings", body: [
                "op": "set_wiki_model",
                "model": model,
                "model_name": modelName,
            ])
            lastWritten[Key.wikiModel] = AnyHashable(model)
            lastWritten[Key.wikiModelName] = AnyHashable(modelName)
            didDispatch = true
        }

        if touched.contains(Key.categorizationModel) || touched.contains(Key.categorizationModelName),
           let model = (kvs.object(forKey: Key.categorizationModel) as? String),
           let modelName = (kvs.object(forKey: Key.categorizationModelName) as? String),
           lastWritten[Key.categorizationModel] != AnyHashable(model)
             || lastWritten[Key.categorizationModelName] != AnyHashable(modelName) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings", body: [
                "op": "set_categorization_model",
                "model": model,
                "model_name": modelName,
            ])
            lastWritten[Key.categorizationModel] = AnyHashable(model)
            lastWritten[Key.categorizationModelName] = AnyHashable(modelName)
            didDispatch = true
        }

        if touched.contains(Key.chapterCompilationModel) || touched.contains(Key.chapterCompilationModelName),
           let model = (kvs.object(forKey: Key.chapterCompilationModel) as? String),
           let modelName = (kvs.object(forKey: Key.chapterCompilationModelName) as? String),
           lastWritten[Key.chapterCompilationModel] != AnyHashable(model)
             || lastWritten[Key.chapterCompilationModelName] != AnyHashable(modelName) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings", body: [
                "op": "set_chapter_compilation_model",
                "model": model,
                "model_name": modelName,
            ])
            lastWritten[Key.chapterCompilationModel] = AnyHashable(model)
            lastWritten[Key.chapterCompilationModelName] = AnyHashable(modelName)
            didDispatch = true
        }

        if touched.contains(Key.embeddingsModel) || touched.contains(Key.embeddingsModelName),
           let model = (kvs.object(forKey: Key.embeddingsModel) as? String),
           let modelName = (kvs.object(forKey: Key.embeddingsModelName) as? String),
           lastWritten[Key.embeddingsModel] != AnyHashable(model)
             || lastWritten[Key.embeddingsModelName] != AnyHashable(modelName) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings", body: [
                "op": "set_embeddings_model",
                "model": model,
                "model_name": modelName,
            ])
            lastWritten[Key.embeddingsModel] = AnyHashable(model)
            lastWritten[Key.embeddingsModelName] = AnyHashable(modelName)
            didDispatch = true
        }

        if touched.contains(Key.imageGenerationModel) || touched.contains(Key.imageGenerationModelName),
           let model = (kvs.object(forKey: Key.imageGenerationModel) as? String),
           let modelName = (kvs.object(forKey: Key.imageGenerationModelName) as? String),
           lastWritten[Key.imageGenerationModel] != AnyHashable(model)
             || lastWritten[Key.imageGenerationModelName] != AnyHashable(modelName) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings", body: [
                "op": "set_image_generation_model",
                "model": model,
                "model_name": modelName,
            ])
            lastWritten[Key.imageGenerationModel] = AnyHashable(model)
            lastWritten[Key.imageGenerationModelName] = AnyHashable(modelName)
            didDispatch = true
        }

        if touched.contains(Key.rerankerEnabled),
           let enabled = (kvs.object(forKey: Key.rerankerEnabled) as? NSNumber)?.boolValue,
           lastWritten[Key.rerankerEnabled] != AnyHashable(enabled) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_reranker_enabled", "enabled": enabled])
            lastWritten[Key.rerankerEnabled] = AnyHashable(enabled)
            didDispatch = true
        }

        if touched.contains(Key.ollamaChatUrl),
           let url = (kvs.object(forKey: Key.ollamaChatUrl) as? String),
           lastWritten[Key.ollamaChatUrl] != AnyHashable(url) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_ollama_chat_url", "url": url])
            lastWritten[Key.ollamaChatUrl] = AnyHashable(url)
            didDispatch = true
        }

        if touched.contains(Key.sttProvider),
           let provider = (kvs.object(forKey: Key.sttProvider) as? String),
           lastWritten[Key.sttProvider] != AnyHashable(provider) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_stt_provider", "provider": provider])
            lastWritten[Key.sttProvider] = AnyHashable(provider)
            didDispatch = true
        }

        if touched.contains(Key.openRouterWhisperModel),
           let model = (kvs.object(forKey: Key.openRouterWhisperModel) as? String),
           lastWritten[Key.openRouterWhisperModel] != AnyHashable(model) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_open_router_whisper_model", "model": model])
            lastWritten[Key.openRouterWhisperModel] = AnyHashable(model)
            didDispatch = true
        }

        if touched.contains(Key.assemblyAiSttModel),
           let model = (kvs.object(forKey: Key.assemblyAiSttModel) as? String),
           lastWritten[Key.assemblyAiSttModel] != AnyHashable(model) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_assembly_ai_stt_model", "model": model])
            lastWritten[Key.assemblyAiSttModel] = AnyHashable(model)
            didDispatch = true
        }

        if touched.contains(Key.elevenLabsSttModel) || touched.contains(Key.elevenLabsTtsModel),
           let sttModel = (kvs.object(forKey: Key.elevenLabsSttModel) as? String),
           let ttsModel = (kvs.object(forKey: Key.elevenLabsTtsModel) as? String),
           lastWritten[Key.elevenLabsSttModel] != AnyHashable(sttModel)
             || lastWritten[Key.elevenLabsTtsModel] != AnyHashable(ttsModel) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings", body: [
                "op": "set_eleven_labs_models",
                "stt_model": sttModel,
                "tts_model": ttsModel,
            ])
            lastWritten[Key.elevenLabsSttModel] = AnyHashable(sttModel)
            lastWritten[Key.elevenLabsTtsModel] = AnyHashable(ttsModel)
            didDispatch = true
        }

        if touched.contains(Key.elevenLabsVoiceId) || touched.contains(Key.elevenLabsVoiceName),
           let voiceId = (kvs.object(forKey: Key.elevenLabsVoiceId) as? String),
           let voiceName = (kvs.object(forKey: Key.elevenLabsVoiceName) as? String),
           lastWritten[Key.elevenLabsVoiceId] != AnyHashable(voiceId)
             || lastWritten[Key.elevenLabsVoiceName] != AnyHashable(voiceName) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings", body: [
                "op": "set_eleven_labs_voice",
                "voice_id": voiceId,
                "voice_name": voiceName,
            ])
            lastWritten[Key.elevenLabsVoiceId] = AnyHashable(voiceId)
            lastWritten[Key.elevenLabsVoiceName] = AnyHashable(voiceName)
            didDispatch = true
        }

        if touched.contains(Key.blossomServerUrl),
           let url = (kvs.object(forKey: Key.blossomServerUrl) as? String),
           lastWritten[Key.blossomServerUrl] != AnyHashable(url) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_blossom_server_url", "url": url])
            lastWritten[Key.blossomServerUrl] = AnyHashable(url)
            didDispatch = true
        }

        if touched.contains(Key.youtubeExtractorUrl),
           let url = (kvs.object(forKey: Key.youtubeExtractorUrl) as? String?) ?? (nil as String?),
           lastWritten[Key.youtubeExtractorUrl] != AnyHashable(url) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_youtube_extractor_url", "url": url as Any])
            lastWritten[Key.youtubeExtractorUrl] = AnyHashable(url)
            didDispatch = true
        }

        if touched.contains(Key.autoMarkPlayedAtEnd),
           let enabled = (kvs.object(forKey: Key.autoMarkPlayedAtEnd) as? NSNumber)?.boolValue,
           lastWritten[Key.autoMarkPlayedAtEnd] != AnyHashable(enabled) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_auto_mark_played_at_end", "enabled": enabled])
            lastWritten[Key.autoMarkPlayedAtEnd] = AnyHashable(enabled)
            didDispatch = true
        }

        if touched.contains(Key.autoPlayNext),
           let enabled = (kvs.object(forKey: Key.autoPlayNext) as? NSNumber)?.boolValue,
           lastWritten[Key.autoPlayNext] != AnyHashable(enabled) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_auto_play_next", "enabled": enabled])
            lastWritten[Key.autoPlayNext] = AnyHashable(enabled)
            didDispatch = true
        }

        if touched.contains(Key.headphoneDoubleTapAction) || touched.contains(Key.headphoneTripleTapAction),
           let doubleTap = (kvs.object(forKey: Key.headphoneDoubleTapAction) as? String),
           let tripleTap = (kvs.object(forKey: Key.headphoneTripleTapAction) as? String),
           lastWritten[Key.headphoneDoubleTapAction] != AnyHashable(doubleTap)
             || lastWritten[Key.headphoneTripleTapAction] != AnyHashable(tripleTap) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings", body: [
                "op": "set_headphone_gesture_actions",
                "double_tap": doubleTap,
                "triple_tap": tripleTap,
            ])
            lastWritten[Key.headphoneDoubleTapAction] = AnyHashable(doubleTap)
            lastWritten[Key.headphoneTripleTapAction] = AnyHashable(tripleTap)
            didDispatch = true
        }

        if touched.contains(Key.wikiAutoGenerateOnTranscriptIngest),
           let enabled = (kvs.object(forKey: Key.wikiAutoGenerateOnTranscriptIngest) as? NSNumber)?.boolValue,
           lastWritten[Key.wikiAutoGenerateOnTranscriptIngest] != AnyHashable(enabled) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_wiki_auto_generate_on_transcript_ingest", "enabled": enabled])
            lastWritten[Key.wikiAutoGenerateOnTranscriptIngest] = AnyHashable(enabled)
            didDispatch = true
        }

        if touched.contains(Key.autoIngestPublisherTranscripts),
           let enabled = (kvs.object(forKey: Key.autoIngestPublisherTranscripts) as? NSNumber)?.boolValue,
           lastWritten[Key.autoIngestPublisherTranscripts] != AnyHashable(enabled) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_auto_ingest_publisher_transcripts", "enabled": enabled])
            lastWritten[Key.autoIngestPublisherTranscripts] = AnyHashable(enabled)
            didDispatch = true
        }

        if touched.contains(Key.autoFallbackToScribe),
           let enabled = (kvs.object(forKey: Key.autoFallbackToScribe) as? NSNumber)?.boolValue,
           lastWritten[Key.autoFallbackToScribe] != AnyHashable(enabled) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_auto_fallback_to_scribe", "enabled": enabled])
            lastWritten[Key.autoFallbackToScribe] = AnyHashable(enabled)
            didDispatch = true
        }

        if touched.contains(Key.notifyOnNewEpisodes),
           let enabled = (kvs.object(forKey: Key.notifyOnNewEpisodes) as? NSNumber)?.boolValue,
           lastWritten[Key.notifyOnNewEpisodes] != AnyHashable(enabled) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_notify_on_new_episodes", "enabled": enabled])
            lastWritten[Key.notifyOnNewEpisodes] = AnyHashable(enabled)
            didDispatch = true
        }

        if touched.contains(Key.nostrRelayUrl),
           let url = (kvs.object(forKey: Key.nostrRelayUrl) as? String),
           lastWritten[Key.nostrRelayUrl] != AnyHashable(url) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_nostr_relay_url", "url": url])
            lastWritten[Key.nostrRelayUrl] = AnyHashable(url)
            didDispatch = true
        }

        if touched.contains(Key.nostrProfileName) || touched.contains(Key.nostrProfileAbout) || touched.contains(Key.nostrProfilePicture),
           let name = (kvs.object(forKey: Key.nostrProfileName) as? String),
           let about = (kvs.object(forKey: Key.nostrProfileAbout) as? String),
           let picture = (kvs.object(forKey: Key.nostrProfilePicture) as? String),
           lastWritten[Key.nostrProfileName] != AnyHashable(name)
             || lastWritten[Key.nostrProfileAbout] != AnyHashable(about)
             || lastWritten[Key.nostrProfilePicture] != AnyHashable(picture) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings", body: [
                "op": "set_nostr_profile",
                "name": name,
                "about": about,
                "picture": picture,
            ])
            lastWritten[Key.nostrProfileName] = AnyHashable(name)
            lastWritten[Key.nostrProfileAbout] = AnyHashable(about)
            lastWritten[Key.nostrProfilePicture] = AnyHashable(picture)
            didDispatch = true
        }

        // Nothing actually dispatched → no kernel echo to suppress; reset
        // the flag so the next outbound tick is not swallowed.
        if !didDispatch {
            isApplyingRemoteChange = false
            appStore?.isApplyingRemoteChange = false
        }
    }
}

// `SettingsKVSnapshot` (the value the snapshot bridge produces) and
// the `from(podcastUpdate:)` bridge live in
// `iCloudSyncCapability+Snapshot.swift`.

#if DEBUG
extension iCloudSyncCapability {
    /// Test-only seam: drive the capability into the started state
    /// without a `KernelModel`. The outbound path checks `started` but
    /// not `kernel`, so this enables outbound-only unit tests that do
    /// not need a live Rust kernel handle. Compiled out of release
    /// builds.
    func testForceStarted() {
        started = true
    }

    /// Test-only seam: directly flip the echo-suppression flag so the
    /// outbound-skip behaviour can be exercised without round-tripping
    /// through an external KVS notification.
    func testSetApplyingRemoteChange(_ value: Bool) {
        isApplyingRemoteChange = value
    }
}
#endif
