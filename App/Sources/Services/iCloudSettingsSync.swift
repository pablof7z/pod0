import Foundation
import Pod0Core
import os.log

// MARK: - iCloudSettingsSync

/// Mirrors a curated subset of `Settings` into `NSUbiquitousKeyValueStore` so
/// that app preferences (models, relay URL, voice config) roam across devices
/// and survive reinstalls.
///
/// **What is synced.** Only portable, non-secret fields:
///   - LLM model IDs / names (agent, memory compilation, utility)
///   - ElevenLabs TTS/STT model IDs, voice ID, and voice name
///   - Playback preferences (default rate, skip intervals, auto-mark-played)
///   - Transcript automation toggles
///   - Per-kind notification toggles
///   - Agent display name and avatar
///
/// **What is NOT synced.** Fields that are device-local, security-sensitive, or
/// bound to entries in the Keychain:
///   - `openRouterCredentialSource`, `*BYOKKeyID/Label`, `*ConnectedAt` — tied to
///     local Keychain secrets; syncing source without syncing the secret is
///     misleading and could make the app appear connected when it isn't
///   - `ollamaCredentialSource`, `ollamaBYOKKeyID/Label`, `ollamaConnectedAt` —
///     same local-Keychain rule
///   - `elevenLabsCredentialSource`, `*BYOKKeyID/Label`, `*ConnectedAt` — same
///     reasoning as above
///
/// iCloud carries values plus version evidence. Rust validates and resolves
/// every conflict before the resulting projection is rendered or mirrored.
@MainActor
final class iCloudSettingsSync {
    nonisolated private static let logger = Logger.app("iCloudSettingsSync")

    // MARK: - Singleton

    static let shared = iCloudSettingsSync()

    // MARK: - Private state

    /// Reference to the underlying key-value store.
    private let kvs = NSUbiquitousKeyValueStore.default

    /// Retained observer token for `NSUbiquitousKeyValueStore` change events.
    private var kvsObserver: NSObjectProtocol?

    // MARK: - Init

    private init() {}

    // MARK: - Lifecycle

    /// Registers the raw transport observer. Product merging remains in Rust.
    func start() {
        guard kvsObserver == nil else { return }
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
        Self.logger.info("iCloudSettingsSync started")
    }

    func push(_ settings: ProductSettings) {
        let portable = ProductSettingsBridge.applying(settings.values, to: Settings())
        write(portable, to: kvs)
        kvs.set(String(settings.schemaVersion), forKey: Key.schemaVersion.rawValue)
        kvs.set(String(settings.writerVersion.counter), forKey: Key.writerCounter.rawValue)
        write(settings.writerVersion.writerId, to: kvs)
    }

    func remoteSnapshot(basedOn settings: Settings) throws -> iCloudProductSettingsSnapshot? {
        guard Key.portable.contains(where: { kvs.object(forKey: $0.rawValue) != nil }) else {
            return nil
        }
        var merged = settings
        merge(from: kvs, into: &merged)
        return iCloudProductSettingsSnapshot(
            schemaVersion: UInt32(string(.schemaVersion) ?? "") ?? 1,
            writerVersion: SettingsWriterVersion(
                counter: UInt64(string(.writerCounter) ?? "") ?? 1,
                writerId: readWriterID(from: kvs)
                    ?? SharedLibraryBootstrap.stableDigest("pod0-legacy-icloud-settings-writer")
            ),
            values: try ProductSettingsBridge.values(from: merged)
        )
    }

    /// Removes the former Swift-owned recall keys only after the shared core
    /// has verified their import. Recall configuration is not an iCloud mirror.
    func retireLegacyRecallConfiguration() {
        kvs.removeObject(forKey: Key.embeddingsModel.rawValue)
        kvs.removeObject(forKey: Key.embeddingsModelName.rawValue)
        kvs.removeObject(forKey: Key.rerankerEnabled.rawValue)
        kvs.synchronize()
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
        if let v = string(.imageGenerationModel), !v.isEmpty  { settings.imageGenerationModel = v }
        if let v = string(.imageGenerationModelName)          { settings.imageGenerationModelName = v }
        if let v = string(.ollamaChatURL), !v.isEmpty          { settings.ollamaChatURL = v }
        if let v = string(.youtubeExtractorURL)                { settings.youtubeExtractorURL = v }
        if let v = string(.embeddingsModel), !v.isEmpty {
            settings.legacyRecallEmbeddingsModel = v
        }
        if let v = string(.embeddingsModelName) {
            settings.legacyRecallEmbeddingsModelName = v
        }
        if let v = bool(.rerankerEnabled) {
            settings.legacyRecallRerankerEnabled = v
        }
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
        if let v = bool(.autoSkipAds)                          { settings.autoSkipAds = v }
        if let raw = string(.headphoneDoubleTapAction),
           let v = HeadphoneGestureAction(rawValue: raw)      { settings.headphoneDoubleTapAction = v }
        if let raw = string(.headphoneTripleTapAction),
           let v = HeadphoneGestureAction(rawValue: raw)      { settings.headphoneTripleTapAction = v }
        if let v = bool(.autoIngestPublisherTranscripts)      { settings.autoIngestPublisherTranscripts = v }
        if let v = bool(.autoFallbackToScribe)                { settings.autoFallbackToScribe = v }
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
        kvs.set(settings.imageGenerationModel,                    forKey: Key.imageGenerationModel.rawValue)
        kvs.set(settings.imageGenerationModelName,                forKey: Key.imageGenerationModelName.rawValue)
        kvs.set(settings.ollamaChatURL,                            forKey: Key.ollamaChatURL.rawValue)
        if let youtubeExtractorURL = settings.youtubeExtractorURL {
            kvs.set(youtubeExtractorURL, forKey: Key.youtubeExtractorURL.rawValue)
        } else {
            kvs.removeObject(forKey: Key.youtubeExtractorURL.rawValue)
        }
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
        kvs.set(settings.autoSkipAds,                              forKey: Key.autoSkipAds.rawValue)
        kvs.set(settings.headphoneDoubleTapAction.rawValue,       forKey: Key.headphoneDoubleTapAction.rawValue)
        kvs.set(settings.headphoneTripleTapAction.rawValue,       forKey: Key.headphoneTripleTapAction.rawValue)
        kvs.set(settings.autoIngestPublisherTranscripts,          forKey: Key.autoIngestPublisherTranscripts.rawValue)
        kvs.set(settings.autoFallbackToScribe,                    forKey: Key.autoFallbackToScribe.rawValue)
        kvs.set(settings.agentDisplayName,                        forKey: Key.agentDisplayName.rawValue)
        kvs.set(settings.agentAvatarURLString,                    forKey: Key.agentAvatarURLString.rawValue)
    }

    private func string(_ key: Key) -> String? {
        kvs.object(forKey: key.rawValue) as? String
    }

    private func write(_ digest: ContentDigest, to kvs: NSUbiquitousKeyValueStore) {
        kvs.set(String(digest.word0), forKey: Key.writerWord0.rawValue)
        kvs.set(String(digest.word1), forKey: Key.writerWord1.rawValue)
        kvs.set(String(digest.word2), forKey: Key.writerWord2.rawValue)
        kvs.set(String(digest.word3), forKey: Key.writerWord3.rawValue)
    }

    private func readWriterID(from kvs: NSUbiquitousKeyValueStore) -> ContentDigest? {
        guard let word0 = string(.writerWord0).flatMap(UInt64.init),
              let word1 = string(.writerWord1).flatMap(UInt64.init),
              let word2 = string(.writerWord2).flatMap(UInt64.init),
              let word3 = string(.writerWord3).flatMap(UInt64.init)
        else { return nil }
        return ContentDigest(word0: word0, word1: word1, word2: word2, word3: word3)
    }
}

// MARK: - Notification name

extension iCloudSettingsSync {
    /// Posted on the main thread when an external iCloud change arrives.
    /// `AppStateStore` observes this to pull the latest values into `state`.
    nonisolated static let settingsDidChangeExternallyNotification =
        Notification.Name("iCloudSettingsSync.settingsDidChangeExternally")
}
