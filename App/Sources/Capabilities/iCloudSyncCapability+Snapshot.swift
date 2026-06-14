import Foundation

// MARK: - SettingsKVSnapshot
//
// Decoupled snapshot of the kernel-side fields the iCloud sync capability
// mirrors. Defined here (not on `SettingsSnapshot`) so the capability does
// not need to wait for `pr-settings-projection` to land the playback fields
// on the projection. The snapshot poll constructs one of these from
// whatever subset of fields the current `SettingsSnapshot` exposes; missing
// fields stay `nil` and are simply skipped on the outbound path.

/// Plain-data view of the subset of settings the iCloud capability mirrors.
/// Fields are optional because the `SettingsSnapshot` projection may not yet
/// carry every field on every binary — a `nil` field is a "kernel hasn't
/// projected this yet, skip" signal.
struct SettingsKVSnapshot: Equatable {
    var speed: Double?
    var skipForwardSecs: Int?
    var skipBackwardSecs: Int?
    var autoSkipAds: Bool?
    var autoDeleteDownloadsAfterPlayed: Bool?
    var agentInitialModel: String?
    var agentInitialModelName: String?
    var agentThinkingModel: String?
    var agentThinkingModelName: String?
    var memoryCompilationModel: String?
    var memoryCompilationModelName: String?
    var wikiModel: String?
    var wikiModelName: String?
    var categorizationModel: String?
    var categorizationModelName: String?
    var chapterCompilationModel: String?
    var chapterCompilationModelName: String?
    var embeddingsModel: String?
    var embeddingsModelName: String?
    var imageGenerationModel: String?
    var imageGenerationModelName: String?
    var rerankerEnabled: Bool?
    var ollamaChatUrl: String?
    var sttProvider: String?
    var openRouterWhisperModel: String?
    var assemblyAiSttModel: String?
    var elevenLabsSttModel: String?
    var elevenLabsTtsModel: String?
    var elevenLabsVoiceId: String?
    var elevenLabsVoiceName: String?
    var blossomServerUrl: String?
    var youtubeExtractorUrl: String??
    var autoMarkPlayedAtEnd: Bool?
    var autoPlayNext: Bool?
    var headphoneDoubleTapAction: String?
    var headphoneTripleTapAction: String?
    var wikiAutoGenerateOnTranscriptIngest: Bool?
    var autoIngestPublisherTranscripts: Bool?
    var autoFallbackToScribe: Bool?
    var notifyOnNewEpisodes: Bool?
    var nostrRelayUrl: String?
    var nostrProfileName: String?
    var nostrProfileAbout: String?
    var nostrProfilePicture: String?

    /// All-`nil` snapshot. Returned by the `PodcastUpdate` bridge when the
    /// active kernel projection does not (yet) carry any of the playback
    /// fields the capability syncs.
    static let empty = SettingsKVSnapshot()

    /// Seed the capability's `lastWritten` map with this snapshot's present
    /// fields. Used during inbound merges so the next outbound diff does
    /// not re-emit the value we just applied.
    func write(to lastWritten: inout [String: AnyHashable]) {
        if let speed {
            lastWritten[iCloudSyncCapability.Key.speed] = AnyHashable(speed)
        }
        if let v = skipForwardSecs {
            lastWritten[iCloudSyncCapability.Key.skipForwardSecs] = AnyHashable(v)
        }
        if let v = skipBackwardSecs {
            lastWritten[iCloudSyncCapability.Key.skipBackwardSecs] = AnyHashable(v)
        }
        if let v = autoSkipAds {
            lastWritten[iCloudSyncCapability.Key.autoSkipAds] = AnyHashable(v)
        }
        if let v = autoDeleteDownloadsAfterPlayed {
            lastWritten[iCloudSyncCapability.Key.autoDeleteDownloadsAfterPlayed] = AnyHashable(v)
        }
        if let v = agentInitialModel {
            lastWritten[iCloudSyncCapability.Key.agentInitialModel] = AnyHashable(v)
        }
        if let v = agentInitialModelName {
            lastWritten[iCloudSyncCapability.Key.agentInitialModelName] = AnyHashable(v)
        }
        if let v = agentThinkingModel {
            lastWritten[iCloudSyncCapability.Key.agentThinkingModel] = AnyHashable(v)
        }
        if let v = agentThinkingModelName {
            lastWritten[iCloudSyncCapability.Key.agentThinkingModelName] = AnyHashable(v)
        }
        if let v = memoryCompilationModel {
            lastWritten[iCloudSyncCapability.Key.memoryCompilationModel] = AnyHashable(v)
        }
        if let v = memoryCompilationModelName {
            lastWritten[iCloudSyncCapability.Key.memoryCompilationModelName] = AnyHashable(v)
        }
        if let v = wikiModel {
            lastWritten[iCloudSyncCapability.Key.wikiModel] = AnyHashable(v)
        }
        if let v = wikiModelName {
            lastWritten[iCloudSyncCapability.Key.wikiModelName] = AnyHashable(v)
        }
        if let v = categorizationModel {
            lastWritten[iCloudSyncCapability.Key.categorizationModel] = AnyHashable(v)
        }
        if let v = categorizationModelName {
            lastWritten[iCloudSyncCapability.Key.categorizationModelName] = AnyHashable(v)
        }
        if let v = chapterCompilationModel {
            lastWritten[iCloudSyncCapability.Key.chapterCompilationModel] = AnyHashable(v)
        }
        if let v = chapterCompilationModelName {
            lastWritten[iCloudSyncCapability.Key.chapterCompilationModelName] = AnyHashable(v)
        }
        if let v = embeddingsModel {
            lastWritten[iCloudSyncCapability.Key.embeddingsModel] = AnyHashable(v)
        }
        if let v = embeddingsModelName {
            lastWritten[iCloudSyncCapability.Key.embeddingsModelName] = AnyHashable(v)
        }
        if let v = imageGenerationModel {
            lastWritten[iCloudSyncCapability.Key.imageGenerationModel] = AnyHashable(v)
        }
        if let v = imageGenerationModelName {
            lastWritten[iCloudSyncCapability.Key.imageGenerationModelName] = AnyHashable(v)
        }
        if let v = rerankerEnabled {
            lastWritten[iCloudSyncCapability.Key.rerankerEnabled] = AnyHashable(v)
        }
        if let v = ollamaChatUrl {
            lastWritten[iCloudSyncCapability.Key.ollamaChatUrl] = AnyHashable(v)
        }
        if let v = sttProvider {
            lastWritten[iCloudSyncCapability.Key.sttProvider] = AnyHashable(v)
        }
        if let v = openRouterWhisperModel {
            lastWritten[iCloudSyncCapability.Key.openRouterWhisperModel] = AnyHashable(v)
        }
        if let v = assemblyAiSttModel {
            lastWritten[iCloudSyncCapability.Key.assemblyAiSttModel] = AnyHashable(v)
        }
        if let v = elevenLabsSttModel {
            lastWritten[iCloudSyncCapability.Key.elevenLabsSttModel] = AnyHashable(v)
        }
        if let v = elevenLabsTtsModel {
            lastWritten[iCloudSyncCapability.Key.elevenLabsTtsModel] = AnyHashable(v)
        }
        if let v = elevenLabsVoiceId {
            lastWritten[iCloudSyncCapability.Key.elevenLabsVoiceId] = AnyHashable(v)
        }
        if let v = elevenLabsVoiceName {
            lastWritten[iCloudSyncCapability.Key.elevenLabsVoiceName] = AnyHashable(v)
        }
        if let v = blossomServerUrl {
            lastWritten[iCloudSyncCapability.Key.blossomServerUrl] = AnyHashable(v)
        }
        if let v = youtubeExtractorUrl {
            lastWritten[iCloudSyncCapability.Key.youtubeExtractorUrl] = AnyHashable(v)
        }
        if let v = autoMarkPlayedAtEnd {
            lastWritten[iCloudSyncCapability.Key.autoMarkPlayedAtEnd] = AnyHashable(v)
        }
        if let v = autoPlayNext {
            lastWritten[iCloudSyncCapability.Key.autoPlayNext] = AnyHashable(v)
        }
        if let v = headphoneDoubleTapAction {
            lastWritten[iCloudSyncCapability.Key.headphoneDoubleTapAction] = AnyHashable(v)
        }
        if let v = headphoneTripleTapAction {
            lastWritten[iCloudSyncCapability.Key.headphoneTripleTapAction] = AnyHashable(v)
        }
        if let v = wikiAutoGenerateOnTranscriptIngest {
            lastWritten[iCloudSyncCapability.Key.wikiAutoGenerateOnTranscriptIngest] = AnyHashable(v)
        }
        if let v = autoIngestPublisherTranscripts {
            lastWritten[iCloudSyncCapability.Key.autoIngestPublisherTranscripts] = AnyHashable(v)
        }
        if let v = autoFallbackToScribe {
            lastWritten[iCloudSyncCapability.Key.autoFallbackToScribe] = AnyHashable(v)
        }
        if let v = notifyOnNewEpisodes {
            lastWritten[iCloudSyncCapability.Key.notifyOnNewEpisodes] = AnyHashable(v)
        }
        if let v = nostrRelayUrl {
            lastWritten[iCloudSyncCapability.Key.nostrRelayUrl] = AnyHashable(v)
        }
        if let v = nostrProfileName {
            lastWritten[iCloudSyncCapability.Key.nostrProfileName] = AnyHashable(v)
        }
        if let v = nostrProfileAbout {
            lastWritten[iCloudSyncCapability.Key.nostrProfileAbout] = AnyHashable(v)
        }
        if let v = nostrProfilePicture {
            lastWritten[iCloudSyncCapability.Key.nostrProfilePicture] = AnyHashable(v)
        }
    }
}

// MARK: - PodcastUpdate bridge
//
// Maps the current kernel `PodcastUpdate` into the subset of fields this
// capability mirrors to iCloud. Every field is sourced from the
// kernel-owned **settings preference** (not from transient state like
// `nowPlaying.speed`, which is the *currently playing* rate and defaults
// to `1.0` every time a new episode loads — pushing it back to iCloud
// would clobber the user's persisted preference).
//
// The settings-projection work lands the four scalar fields on
// `SettingsSnapshot` in `pr-settings-projection`. Until those fields
// exist on the generated type this extension returns `.empty` so the
// outbound path is a guaranteed no-op. When the projection lands, this
// extension is the **only** site that needs to change — the capability
// itself is already wired.

extension SettingsKVSnapshot {
    /// Build a snapshot from the current kernel `PodcastUpdate`. Returns
    /// `.empty` when the active `PodcastUpdate` does not yet carry the
    /// playback-rate / skip-interval / auto-skip-ads fields on `settings`
    /// (the pre-`pr-settings-projection` shape).
    ///
    /// When the projection lands, replace the body with explicit reads
    /// from the **preference** fields (not from `nowPlaying.speed` —
    /// see the explanation above):
    ///
    /// ```swift
    /// SettingsKVSnapshot(
    ///     speed: update.settings.playbackSpeed,
    ///     skipForwardSecs: update.settings.skipForwardSecs,
    ///     skipBackwardSecs: update.settings.skipBackwardSecs,
    ///     autoSkipAds: update.settings.autoSkipAds)
    /// ```
    static func from(podcastUpdate update: PodcastUpdate) -> SettingsKVSnapshot {
        let s = update.settings
        return SettingsKVSnapshot(
            speed: s.defaultPlaybackRate,
            skipForwardSecs: Int(s.skipForwardSecs),
            skipBackwardSecs: Int(s.skipBackwardSecs),
            autoSkipAds: s.autoSkipAdsEnabled,
            autoDeleteDownloadsAfterPlayed: s.autoDeleteDownloadsAfterPlayed,
            agentInitialModel: s.agentInitialModel,
            agentInitialModelName: s.agentInitialModelName,
            agentThinkingModel: s.agentThinkingModel,
            agentThinkingModelName: s.agentThinkingModelName,
            memoryCompilationModel: s.memoryCompilationModel,
            memoryCompilationModelName: s.memoryCompilationModelName,
            wikiModel: s.wikiModel,
            wikiModelName: s.wikiModelName,
            categorizationModel: s.categorizationModel,
            categorizationModelName: s.categorizationModelName,
            chapterCompilationModel: s.chapterCompilationModel,
            chapterCompilationModelName: s.chapterCompilationModelName,
            embeddingsModel: s.embeddingsModel,
            embeddingsModelName: s.embeddingsModelName,
            imageGenerationModel: s.imageGenerationModel,
            imageGenerationModelName: s.imageGenerationModelName,
            rerankerEnabled: s.rerankerEnabled,
            ollamaChatUrl: s.ollamaChatURL,
            sttProvider: s.sttProvider,
            openRouterWhisperModel: s.openRouterWhisperModel,
            assemblyAiSttModel: s.assemblyAISTTModel,
            elevenLabsSttModel: s.elevenLabsSTTModel,
            elevenLabsTtsModel: s.elevenLabsTTSModel,
            elevenLabsVoiceId: s.elevenLabsVoiceID,
            elevenLabsVoiceName: s.elevenLabsVoiceName,
            blossomServerUrl: s.blossomServerURL,
            youtubeExtractorUrl: s.youtubeExtractorURL,
            autoMarkPlayedAtEnd: s.autoMarkPlayedAtEnd,
            autoPlayNext: s.autoPlayNext,
            headphoneDoubleTapAction: s.headphoneDoubleTapAction,
            headphoneTripleTapAction: s.headphoneTripleTapAction,
            wikiAutoGenerateOnTranscriptIngest: s.wikiAutoGenerateOnTranscriptIngest,
            autoIngestPublisherTranscripts: s.autoIngestPublisherTranscripts,
            autoFallbackToScribe: s.autoFallbackToScribe,
            notifyOnNewEpisodes: s.notifyOnNewEpisodes,
            nostrRelayUrl: s.nostrRelayURL,
            nostrProfileName: s.nostrProfileName,
            nostrProfileAbout: s.nostrProfileAbout,
            nostrProfilePicture: s.nostrProfilePicture)
    }
}
