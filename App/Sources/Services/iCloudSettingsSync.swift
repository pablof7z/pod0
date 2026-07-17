import Foundation
import os.log

// MARK: - iCloudSettingsSync

/// Mirrors a curated subset of `Settings` into `NSUbiquitousKeyValueStore` so
/// that app preferences (models, relay URL, voice config) roam across devices
/// and survive reinstalls.
///
/// **What is synced.** Only portable, non-secret fields:
///   - LLM model IDs / names (agent, memory compilation, utility, embeddings)
///   - Reranker preference
///   - ElevenLabs TTS/STT model IDs, voice ID, and voice name
///   - Playback preferences (default rate, skip intervals, auto-mark-played)
///   - Transcript automation toggles
///   - Per-kind notification toggles
///   - Agent display name and avatar
///
/// **What is NOT synced.** Fields that are device-local, security-sensitive, or
/// bound to entries in the Keychain:
///   - `hasCompletedOnboarding` — local UX gate; reinstall should show onboarding
///   - `openRouterCredentialSource`, `*BYOKKeyID/Label`, `*ConnectedAt` — tied to
///     local Keychain secrets; syncing source without syncing the secret is
///     misleading and could make the app appear connected when it isn't
///   - `ollamaCredentialSource`, `ollamaBYOKKeyID/Label`, `ollamaConnectedAt` —
///     same local-Keychain rule
///   - `elevenLabsCredentialSource`, `*BYOKKeyID/Label`, `*ConnectedAt` — same
///     reasoning as above
///
/// **Conflict resolution.** `NSUbiquitousKeyValueStore` uses last-write-wins
/// across devices. On first launch after reinstall (or first launch on a new
/// device) an explicit merge call prefers iCloud values over the local defaults
/// so that model preferences are immediately available.
///
/// **Loop prevention.** The `isApplyingRemoteChange` flag blocks the outbound
/// writer while an inbound merge is in progress so that updating `state.settings`
/// does not immediately re-echo the same values back to iCloud.
@MainActor
final class iCloudSettingsSync {
    nonisolated private static let logger = Logger.app("iCloudSettingsSync")

    // MARK: - Singleton

    static let shared = iCloudSettingsSync()

    // MARK: - Private state

    /// Guards against echo-back: set to `true` while applying an inbound
    /// change so the outbound path skips the write.
    var isApplyingRemoteChange = false

    /// Reference to the underlying key-value store.
    private let kvs = NSUbiquitousKeyValueStore.default

    /// Retained observer token for `NSUbiquitousKeyValueStore` change events.
    private var kvsObserver: NSObjectProtocol?

    // MARK: - Init

    private init() {}

    // MARK: - Lifecycle

    /// Registers the notification observer and performs an initial merge so
    /// that iCloud values are reflected before the first view renders.
    ///
    /// Call once from `AppStateStore.init`, passing the freshly loaded state.
    /// The sync service merges iCloud values in-place; `AppStateStore` should
    /// assign the mutated settings to `state` before presenting any UI.
    func start(mergingInto settings: inout Settings) {
        kvsObserver = NotificationCenter.default.addObserver(
            forName: NSUbiquitousKeyValueStore.didChangeExternallyNotification,
            object: kvs,
            queue: .main
        ) { _ in
            // Re-post on the main queue under our own notification name so
            // AppStateStore can observe it without importing Foundation's KVS.
            iCloudSettingsSync.logger.info("iCloudSettingsSync: external change received")
            NotificationCenter.default.post(
                name: iCloudSettingsSync.settingsDidChangeExternallyNotification,
                object: nil
            )
        }
        // Kick off a background fetch from iCloud.
        kvs.synchronize()
        // One-time merge: prefer stored iCloud values over local defaults.
        merge(from: kvs, into: &settings)
        Self.logger.info("iCloudSettingsSync started")
    }

    /// Pushes current settings to the key-value store.
    /// Call whenever settings change (i.e. from `AppStateStore.state.didSet`).
    ///
    /// Does nothing when an inbound change is being applied — prevents loops.
    func push(_ settings: Settings) {
        guard !isApplyingRemoteChange else { return }
        write(settings, to: kvs)
    }

    // MARK: - Merge helper

    /// Applies iCloud values to `settings` for every tracked key that has a
    /// stored value. Keys absent from iCloud are left untouched so local
    /// defaults survive.
    func merge(from kvs: NSUbiquitousKeyValueStore, into settings: inout Settings) {
        func string(_ key: Key) -> String? {
            kvs.object(forKey: key.rawValue) as? String
        }
        func bool(_ key: Key) -> Bool? {
            // `object(forKey:)` returns nil when the key is absent (so we don't
            // overwrite local defaults with `false`); cast to NSNumber to
            // distinguish "not set" from "explicitly false".
            (kvs.object(forKey: key.rawValue) as? NSNumber)?.boolValue
        }
        func double(_ key: Key) -> Double? {
            (kvs.object(forKey: key.rawValue) as? NSNumber)?.doubleValue
        }
        func int(_ key: Key) -> Int? {
            (kvs.object(forKey: key.rawValue) as? NSNumber)?.intValue
        }

        if let v = string(.agentInitialModel),     !v.isEmpty { settings.agentInitialModel = v }
        if let v = string(.agentInitialModelName)             { settings.agentInitialModelName = v }
        if let v = string(.agentThinkingModel),    !v.isEmpty { settings.agentThinkingModel = v }
        if let v = string(.agentThinkingModelName)            { settings.agentThinkingModelName = v }
        if let v = string(.memoryCompilationModel), !v.isEmpty { settings.memoryCompilationModel = v }
        if let v = string(.memoryCompilationModelName)        { settings.memoryCompilationModelName = v }
        if let v = string(.wikiModel),             !v.isEmpty { settings.wikiModel = v }
        if let v = string(.wikiModelName)                     { settings.wikiModelName = v }
        if let v = string(.categorizationModel),   !v.isEmpty { settings.categorizationModel = v }
        if let v = string(.categorizationModelName)           { settings.categorizationModelName = v }
        if let v = string(.chapterCompilationModel), !v.isEmpty { settings.chapterCompilationModel = v }
        if let v = string(.chapterCompilationModelName)       { settings.chapterCompilationModelName = v }
        if let v = string(.embeddingsModel),       !v.isEmpty { settings.embeddingsModel = v }
        if let v = string(.embeddingsModelName)               { settings.embeddingsModelName = v }
        if let v = bool(.rerankerEnabled)                     { settings.rerankerEnabled = v }
        if let raw = string(.sttProvider),
           let v = STTProvider(rawValue: raw)                  { settings.sttProvider = v }
        if let v = string(.openRouterWhisperModel), !v.isEmpty { settings.openRouterWhisperModel = v }
        if let v = string(.assemblyAISTTModel),     !v.isEmpty { settings.assemblyAISTTModel = v }
        if let v = string(.elevenLabsSTTModel),    !v.isEmpty { settings.elevenLabsSTTModel = v }
        if let v = string(.elevenLabsTTSModel),    !v.isEmpty { settings.elevenLabsTTSModel = v }
        if let v = string(.elevenLabsVoiceID)                 { settings.elevenLabsVoiceID = v }
        if let v = string(.elevenLabsVoiceName)               { settings.elevenLabsVoiceName = v }
        if let v = double(.defaultPlaybackRate), v > 0        { settings.defaultPlaybackRate = v }
        if let v = int(.skipForwardSeconds), v > 0            { settings.skipForwardSeconds = v }
        if let v = int(.skipBackwardSeconds), v > 0           { settings.skipBackwardSeconds = v }
        if let v = bool(.autoMarkPlayedAtEnd)                 { settings.autoMarkPlayedAtEnd = v }
        if let v = bool(.autoPlayNext)                        { settings.autoPlayNext = v }
        if let v = bool(.autoDeleteDownloadsAfterPlayed)      { settings.autoDeleteDownloadsAfterPlayed = v }
        if let raw = string(.headphoneDoubleTapAction),
           let v = HeadphoneGestureAction(rawValue: raw)      { settings.headphoneDoubleTapAction = v }
        if let raw = string(.headphoneTripleTapAction),
           let v = HeadphoneGestureAction(rawValue: raw)      { settings.headphoneTripleTapAction = v }
        if let v = bool(.autoIngestPublisherTranscripts)      { settings.autoIngestPublisherTranscripts = v }
        if let v = bool(.autoFallbackToScribe)                { settings.autoFallbackToScribe = v }
        if let v = bool(.notifyOnNewEpisodes)                 { settings.notifyOnNewEpisodes = v }
        if let v = string(.agentDisplayName)                  { settings.agentDisplayName = v }
        if let v = string(.agentAvatarURLString)              { settings.agentAvatarURLString = v }
    }

    // MARK: - Write helper

    private func write(_ settings: Settings, to kvs: NSUbiquitousKeyValueStore) {
        kvs.set(settings.agentInitialModel,                       forKey: Key.agentInitialModel.rawValue)
        kvs.set(settings.agentInitialModelName,                   forKey: Key.agentInitialModelName.rawValue)
        kvs.set(settings.agentThinkingModel,                      forKey: Key.agentThinkingModel.rawValue)
        kvs.set(settings.agentThinkingModelName,                  forKey: Key.agentThinkingModelName.rawValue)
        kvs.set(settings.memoryCompilationModel,                  forKey: Key.memoryCompilationModel.rawValue)
        kvs.set(settings.memoryCompilationModelName,              forKey: Key.memoryCompilationModelName.rawValue)
        kvs.set(settings.wikiModel,                               forKey: Key.wikiModel.rawValue)
        kvs.set(settings.wikiModelName,                           forKey: Key.wikiModelName.rawValue)
        kvs.set(settings.categorizationModel,                     forKey: Key.categorizationModel.rawValue)
        kvs.set(settings.categorizationModelName,                 forKey: Key.categorizationModelName.rawValue)
        kvs.set(settings.chapterCompilationModel,                 forKey: Key.chapterCompilationModel.rawValue)
        kvs.set(settings.chapterCompilationModelName,             forKey: Key.chapterCompilationModelName.rawValue)
        kvs.set(settings.embeddingsModel,                         forKey: Key.embeddingsModel.rawValue)
        kvs.set(settings.embeddingsModelName,                     forKey: Key.embeddingsModelName.rawValue)
        kvs.set(settings.rerankerEnabled,                         forKey: Key.rerankerEnabled.rawValue)
        kvs.set(settings.sttProvider.rawValue,                    forKey: Key.sttProvider.rawValue)
        kvs.set(settings.openRouterWhisperModel,                  forKey: Key.openRouterWhisperModel.rawValue)
        kvs.set(settings.assemblyAISTTModel,                      forKey: Key.assemblyAISTTModel.rawValue)
        kvs.set(settings.elevenLabsSTTModel,                      forKey: Key.elevenLabsSTTModel.rawValue)
        kvs.set(settings.elevenLabsTTSModel,                      forKey: Key.elevenLabsTTSModel.rawValue)
        kvs.set(settings.elevenLabsVoiceID,                       forKey: Key.elevenLabsVoiceID.rawValue)
        kvs.set(settings.elevenLabsVoiceName,                     forKey: Key.elevenLabsVoiceName.rawValue)
        kvs.set(settings.defaultPlaybackRate,                     forKey: Key.defaultPlaybackRate.rawValue)
        kvs.set(Int64(settings.skipForwardSeconds),               forKey: Key.skipForwardSeconds.rawValue)
        kvs.set(Int64(settings.skipBackwardSeconds),              forKey: Key.skipBackwardSeconds.rawValue)
        kvs.set(settings.autoMarkPlayedAtEnd,                     forKey: Key.autoMarkPlayedAtEnd.rawValue)
        kvs.set(settings.autoPlayNext,                            forKey: Key.autoPlayNext.rawValue)
        kvs.set(settings.autoDeleteDownloadsAfterPlayed,          forKey: Key.autoDeleteDownloadsAfterPlayed.rawValue)
        kvs.set(settings.headphoneDoubleTapAction.rawValue,       forKey: Key.headphoneDoubleTapAction.rawValue)
        kvs.set(settings.headphoneTripleTapAction.rawValue,       forKey: Key.headphoneTripleTapAction.rawValue)
        kvs.set(settings.autoIngestPublisherTranscripts,          forKey: Key.autoIngestPublisherTranscripts.rawValue)
        kvs.set(settings.autoFallbackToScribe,                    forKey: Key.autoFallbackToScribe.rawValue)
        kvs.set(settings.notifyOnNewEpisodes,                     forKey: Key.notifyOnNewEpisodes.rawValue)
        kvs.set(settings.agentDisplayName,                        forKey: Key.agentDisplayName.rawValue)
        kvs.set(settings.agentAvatarURLString,                    forKey: Key.agentAvatarURLString.rawValue)
    }

    // MARK: - Key namespace

    /// Namespaced keys for `NSUbiquitousKeyValueStore` to avoid collisions
    /// with any other KV store entries.
    enum Key: String {
        // RawValues preserved as "sync.settings.llmModel" / "llmModelName" so
        // existing iCloud KVS entries continue to roundtrip after the rename.
        case agentInitialModel                   = "sync.settings.llmModel"
        case agentInitialModelName               = "sync.settings.llmModelName"
        case agentThinkingModel                  = "sync.settings.agentThinkingModel"
        case agentThinkingModelName              = "sync.settings.agentThinkingModelName"
        case memoryCompilationModel              = "sync.settings.memoryCompilationModel"
        case memoryCompilationModelName          = "sync.settings.memoryCompilationModelName"
        case wikiModel                           = "sync.settings.wikiModel"
        case wikiModelName                       = "sync.settings.wikiModelName"
        case categorizationModel                 = "sync.settings.categorizationModel"
        case categorizationModelName             = "sync.settings.categorizationModelName"
        case chapterCompilationModel             = "sync.settings.chapterCompilationModel"
        case chapterCompilationModelName         = "sync.settings.chapterCompilationModelName"
        case embeddingsModel                     = "sync.settings.embeddingsModel"
        case embeddingsModelName                 = "sync.settings.embeddingsModelName"
        case rerankerEnabled                     = "sync.settings.rerankerEnabled"
        case sttProvider                         = "sync.settings.sttProvider"
        case openRouterWhisperModel              = "sync.settings.openRouterWhisperModel"
        case assemblyAISTTModel                  = "sync.settings.assemblyAISTTModel"
        case elevenLabsSTTModel                  = "sync.settings.elevenLabsSTTModel"
        case elevenLabsTTSModel                  = "sync.settings.elevenLabsTTSModel"
        case elevenLabsVoiceID                   = "sync.settings.elevenLabsVoiceID"
        case elevenLabsVoiceName                 = "sync.settings.elevenLabsVoiceName"
        case defaultPlaybackRate                 = "sync.settings.defaultPlaybackRate"
        case skipForwardSeconds                  = "sync.settings.skipForwardSeconds"
        case skipBackwardSeconds                 = "sync.settings.skipBackwardSeconds"
        case autoMarkPlayedAtEnd                 = "sync.settings.autoMarkPlayedAtEnd"
        case autoPlayNext                        = "sync.settings.autoPlayNext"
        case autoDeleteDownloadsAfterPlayed      = "sync.settings.autoDeleteDownloadsAfterPlayed"
        case headphoneDoubleTapAction            = "sync.settings.headphoneDoubleTapAction"
        case headphoneTripleTapAction            = "sync.settings.headphoneTripleTapAction"
        case autoIngestPublisherTranscripts      = "sync.settings.autoIngestPublisherTranscripts"
        case autoFallbackToScribe                = "sync.settings.autoFallbackToScribe"
        case notifyOnNewEpisodes                 = "sync.settings.notifyOnNewEpisodes"
        // RawValues preserved so existing iCloud KVS entries continue to
        // roundtrip after the Nostr identity removal renamed these fields.
        case agentDisplayName                    = "sync.settings.nostrProfileName"
        case agentAvatarURLString                = "sync.settings.nostrProfilePicture"
    }
}

// MARK: - Notification name

extension iCloudSettingsSync {
    /// Posted on the main thread when an external iCloud change arrives.
    /// `AppStateStore` observes this to pull the latest values into `state`.
    nonisolated static let settingsDidChangeExternallyNotification =
        Notification.Name("iCloudSettingsSync.settingsDidChangeExternally")
}
