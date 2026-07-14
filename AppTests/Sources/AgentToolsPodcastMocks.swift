import Foundation
@testable import Podcastr

// MARK: - Lane-10 test mocks
//
// Pulled out of `AgentToolsPodcastTests.swift` so that file stays under the
// 500-line hard limit. Every mock is `actor` so it satisfies the `Sendable`
// protocol surface in `PodcastAgentToolDeps`.

actor MockRAG: PodcastAgentRAGSearchProtocol {
    private let searchResult: [EpisodeHit]
    private let transcriptsResult: [TranscriptHit]
    private let similarResult: [EpisodeHit]
    private(set) var lastSearchLimit: Int = -1
    private(set) var lastSimilarK: Int = -1

    init(
        searchEpisodesResult: [EpisodeHit] = [],
        transcriptsResult: [TranscriptHit] = [],
        similarResult: [EpisodeHit] = []
    ) {
        self.searchResult = searchEpisodesResult
        self.transcriptsResult = transcriptsResult
        self.similarResult = similarResult
    }

    func searchEpisodes(query: String, scope: PodcastID?, limit: Int) async throws -> [EpisodeHit] {
        lastSearchLimit = limit
        return searchResult
    }

    func queryTranscripts(query: String, scope: String?, limit: Int) async throws -> [TranscriptHit] {
        return transcriptsResult
    }

    func findSimilarEpisodes(seedEpisodeID: EpisodeID, k: Int) async throws -> [EpisodeHit] {
        lastSimilarK = k
        return similarResult
    }
}

actor MockWiki: WikiStorageProtocol {
    private let result: [WikiHit]
    private let createResult: WikiCreateResult?
    private let listResult: [WikiPageListing]
    init(
        result: [WikiHit] = [],
        createResult: WikiCreateResult? = nil,
        listResult: [WikiPageListing] = []
    ) {
        self.result = result
        self.createResult = createResult
        self.listResult = listResult
    }

    func queryWiki(topic: String, scope: PodcastID?, limit: Int) async throws -> [WikiHit] {
        return result
    }

    func createWikiPage(title: String, kind: String, scope: PodcastID?) async throws -> WikiCreateResult {
        return createResult ?? WikiCreateResult(
            pageID: "mock-page",
            slug: title.lowercased().replacingOccurrences(of: " ", with: "-"),
            title: title,
            kind: kind,
            summary: "",
            claimCount: 0,
            citationCount: 0,
            confidence: 0
        )
    }

    func listWikiPages(scope: PodcastID?, limit: Int) async throws -> [WikiPageListing] {
        return listResult
    }

    func deleteWikiPage(slug: String, scope: PodcastID?) async throws {}
}

actor MockBriefing: BriefingComposerProtocol {
    private let result: BriefingResult?
    private let error: Error?
    private(set) var lastLength: Int = -1

    init(result: BriefingResult? = nil, error: Error? = nil) {
        self.result = result
        self.error = error
    }

    func composeBriefing(scope: String, lengthMinutes: Int, style: String?) async throws -> BriefingResult {
        lastLength = lengthMinutes
        if let error = error { throw error }
        return result ?? BriefingResult(
            briefingID: "default", title: "Default", estimatedSeconds: 0, episodeIDs: []
        )
    }
}

actor MockSummarizer: EpisodeSummarizerProtocol {
    private let result: EpisodeSummary?
    init(result: EpisodeSummary? = nil) { self.result = result }

    func summarizeEpisode(episodeID: EpisodeID, length: String?) async throws -> EpisodeSummary {
        return result ?? EpisodeSummary(episodeID: episodeID, summary: "")
    }
}

actor MockFetcher: EpisodeFetcherProtocol {
    private let known: Set<EpisodeID>
    init(known: [EpisodeID] = []) { self.known = Set(known) }

    func episodeExists(episodeID: EpisodeID) async -> Bool {
        return known.contains(episodeID)
    }

    func episodeMetadata(episodeID: EpisodeID) async -> (podcastTitle: String, episodeTitle: String, durationSeconds: Int?)? {
        guard known.contains(episodeID) else { return nil }
        return ("Mock Show", "Episode \(episodeID)", 1800)
    }

    func episodeIDForAudioURL(_ audioURLString: String, podcastID: PodcastID) async -> EpisodeID? {
        return nil
    }
}

actor MockPlayback: PlaybackHostProtocol {
    struct RecordedPlay: Equatable {
        let episodeID: EpisodeID
        let startSeconds: Double?
        let endSeconds: Double?
        let queuePosition: QueuePosition
    }
    struct RecordedExternalPlay: Equatable {
        let audioURL: URL
        let title: String
        let feedURLString: String?
        let startSeconds: Double?
        let endSeconds: Double?
        let queuePosition: QueuePosition
    }

    private(set) var recordedPlays: [RecordedPlay] = []
    private(set) var recordedExternalPlays: [RecordedExternalPlay] = []
    private(set) var pauseCount = 0
    private(set) var recordedRates: [Double] = []
    private(set) var recordedSleepTimers: [(String, Int?)] = []

    func playEpisode(
        episodeID: EpisodeID,
        startSeconds: Double?,
        endSeconds: Double?,
        queuePosition: QueuePosition
    ) async -> PlayEpisodeResult? {
        recordedPlays.append(RecordedPlay(
            episodeID: episodeID,
            startSeconds: startSeconds,
            endSeconds: endSeconds,
            queuePosition: queuePosition
        ))
        return PlayEpisodeResult(
            episodeID: episodeID,
            queuePosition: queuePosition,
            startedPlaying: queuePosition == .now,
            episodeTitle: "Mock Episode",
            podcastTitle: "Mock Show",
            durationSeconds: 1800
        )
    }

    func pausePlayback() async -> Bool {
        pauseCount += 1
        return true
    }

    func setPlaybackRate(_ rate: Double) async -> Double? {
        recordedRates.append(rate)
        return min(max(rate, 0.5), 3.0)
    }

    func setSleepTimer(mode: String, minutes: Int?) async -> String? {
        recordedSleepTimers.append((mode, minutes))
        switch mode {
        case "off": return "Off"
        case "end_of_episode": return "End of episode"
        case "minutes": return "\(minutes ?? 0) min"
        default: return "Unknown"
        }
    }

    func playExternalEpisode(
        audioURL: URL,
        title: String,
        feedURLString: String?,
        durationSeconds: TimeInterval?,
        startSeconds: Double?,
        endSeconds: Double?,
        queuePosition: QueuePosition
    ) async -> PlayEpisodeResult? {
        recordedExternalPlays.append(RecordedExternalPlay(
            audioURL: audioURL,
            title: title,
            feedURLString: feedURLString,
            startSeconds: startSeconds,
            endSeconds: endSeconds,
            queuePosition: queuePosition
        ))
        return PlayEpisodeResult(
            episodeID: UUID().uuidString,
            queuePosition: queuePosition,
            startedPlaying: queuePosition == .now,
            episodeTitle: title,
            podcastTitle: nil,
            durationSeconds: durationSeconds.map { Int($0) }
        )
    }
}

actor MockLibrary: PodcastLibraryProtocol {
    private(set) var playedIDs: [EpisodeID] = []
    private(set) var unplayedIDs: [EpisodeID] = []
    private(set) var downloadedIDs: [EpisodeID] = []
    private(set) var transcriptionIDs: [EpisodeID] = []
    private(set) var refreshedPodcastIDs: [PodcastID] = []

    struct ClipCall: Equatable {
        let episodeID: EpisodeID
        let startSeconds: Double
        let endSeconds: Double
        let caption: String?
        let transcriptText: String?
    }
    private(set) var clipCalls: [ClipCall] = []

    func markEpisodePlayed(episodeID: EpisodeID) async throws -> EpisodeMutationResult {
        playedIDs.append(episodeID)
        return episodeResult(episodeID: episodeID, state: "played")
    }

    func markEpisodeUnplayed(episodeID: EpisodeID) async throws -> EpisodeMutationResult {
        unplayedIDs.append(episodeID)
        return episodeResult(episodeID: episodeID, state: "unplayed")
    }

    func downloadEpisode(episodeID: EpisodeID) async throws -> EpisodeMutationResult {
        downloadedIDs.append(episodeID)
        return episodeResult(episodeID: episodeID, state: "downloading")
    }

    func requestTranscription(episodeID: EpisodeID) async throws -> TranscriptRequestResult {
        transcriptionIDs.append(episodeID)
        return TranscriptRequestResult(episodeID: episodeID, status: "queued")
    }

    func downloadAndTranscribe(episodeID: EpisodeID) async throws -> TranscriptRequestResult {
        downloadedIDs.append(episodeID)
        transcriptionIDs.append(episodeID)
        return TranscriptRequestResult(episodeID: episodeID, status: "ready", source: "mock")
    }

    func refreshFeed(podcastID: PodcastID) async throws -> FeedRefreshResult {
        refreshedPodcastIDs.append(podcastID)
        return FeedRefreshResult(
            podcastID: podcastID,
            title: "Mock Show",
            episodeCount: 42,
            newEpisodeCount: 2
        )
    }

    func createClip(
        episodeID: EpisodeID,
        startSeconds: Double,
        endSeconds: Double,
        caption: String?,
        transcriptText: String?
    ) async throws -> ClipResult {
        clipCalls.append(ClipCall(
            episodeID: episodeID,
            startSeconds: startSeconds,
            endSeconds: endSeconds,
            caption: caption,
            transcriptText: transcriptText
        ))
        return ClipResult(
            clipID: "mock-clip-1",
            episodeID: episodeID,
            podcastID: "pod1",
            episodeTitle: "Episode \(episodeID)",
            startSeconds: startSeconds,
            endSeconds: endSeconds,
            transcriptText: transcriptText ?? "",
            caption: caption
        )
    }

    private func episodeResult(episodeID: EpisodeID, state: String) -> EpisodeMutationResult {
        EpisodeMutationResult(
            episodeID: episodeID,
            podcastID: "pod1",
            episodeTitle: "Episode \(episodeID)",
            podcastTitle: "Mock Show",
            state: state
        )
    }
}

actor MockPeerEventPublisher: PeerEventPublisherProtocol {
    struct ConversationReplyCall: Equatable {
        let peerContext: PeerConversationContext
        let body: String
        let extraTags: [[String]]
    }
    struct FriendMessageCall: Equatable {
        let friendPubkeyHex: String
        let body: String
        let peerContext: PeerConversationContext?
    }
    private(set) var conversationReplies: [ConversationReplyCall] = []
    private(set) var friendMessages: [FriendMessageCall] = []
    private var nextEventID = 1
    private let shouldThrow: Bool

    init(shouldThrow: Bool = false) {
        self.shouldThrow = shouldThrow
    }

    func publishConversationReply(
        peerContext: PeerConversationContext,
        body: String,
        extraTags: [[String]]
    ) async throws -> String {
        if shouldThrow { throw NostrEventPublisherError.noRelayConfigured }
        conversationReplies.append(.init(peerContext: peerContext, body: body, extraTags: extraTags))
        defer { nextEventID += 1 }
        return "peer-reply-\(nextEventID)"
    }

    func publishFriendMessage(
        friendPubkeyHex: String,
        body: String,
        peerContext: PeerConversationContext?
    ) async throws -> String {
        if shouldThrow { throw NostrEventPublisherError.noRelayConfigured }
        friendMessages.append(.init(friendPubkeyHex: friendPubkeyHex, body: body, peerContext: peerContext))
        defer { nextEventID += 1 }
        return "friend-msg-\(nextEventID)"
    }
}

actor MockFriendDirectory: FriendDirectoryProtocol {
    private let knownPubkeys: [String]

    init(knownPubkeys: [String] = []) {
        self.knownPubkeys = knownPubkeys.map { $0.lowercased() }
    }

    func resolvePubkey(prefixOrFull: String) async -> String? {
        let needle = prefixOrFull.lowercased()
        return knownPubkeys.first { $0.hasPrefix(needle) }
    }
}

actor MockTTSPublisher: TTSPublisherProtocol {
    nonisolated func defaultVoiceID() -> String { "mock-voice" }
    nonisolated func setDefaultVoiceID(_ voiceID: String) {}

    func generateAndPublish(
        title: String,
        description: String?,
        turns: [TTSTurn],
        playNow: Bool,
        generationSource: Episode.GenerationSource?,
        targetPodcastID: UUID?
    ) async throws -> TTSEpisodeResult {
        return TTSEpisodeResult(
            episodeID: "mock-tts-episode",
            podcastID: "mock-tts-podcast",
            title: title,
            durationSeconds: nil,
            publishedToLibrary: true
        )
    }
}

// `MockDirectory` and `MockSubscribe` live in
// `AgentToolsPodcastMocks+Directory.swift` to keep this file under the
// 500-line cap from `AGENTS.md`.

actor MockPerplexity: PerplexityClientProtocol {
    private let result: PerplexityResult?
    private let error: Error?

    init(result: PerplexityResult? = nil, error: Error? = nil) {
        self.result = result
        self.error = error
    }

    func search(query: String) async throws -> PerplexityResult {
        if let error = error { throw error }
        return result ?? PerplexityResult(answer: "", sources: [])
    }
}

actor MockInventory: PodcastInventoryProtocol, PodcastCategoryProtocol {
    var subscriptions: [SubscriptionSummary] = []
    var episodesByPodcast: [PodcastID: [EpisodeInventoryRow]] = [:]
    var inProgress: [EpisodeInventoryRow] = []
    var recentUnplayed: [EpisodeInventoryRow] = []
    var categories: [PodcastCategorySummary] = []
    var categoryChangeResult: PodcastCategoryChangeResult?
    private(set) var lastListSubscriptionsLimit: Int = -1
    private(set) var lastListEpisodesPodcastID: PodcastID?
    private(set) var lastListEpisodesLimit: Int = -1
    private(set) var lastInProgressLimit: Int = -1
    private(set) var lastRecentUnplayedLimit: Int = -1
    private(set) var lastListCategoriesLimit: Int = -1
    private(set) var lastListCategoriesIncludePodcasts: Bool?
    private(set) var lastCategoryChangePodcastID: PodcastID?
    private(set) var lastCategoryChangeReference: PodcastCategoryReference?

    func setSubscriptions(_ subs: [SubscriptionSummary]) { subscriptions = subs }
    func setEpisodes(_ rows: [EpisodeInventoryRow], forPodcast podcastID: PodcastID) {
        episodesByPodcast[podcastID] = rows
    }
    func setInProgress(_ rows: [EpisodeInventoryRow]) { inProgress = rows }
    func setRecentUnplayed(_ rows: [EpisodeInventoryRow]) { recentUnplayed = rows }
    func setCategories(_ rows: [PodcastCategorySummary]) { categories = rows }
    func setCategoryChangeResult(_ result: PodcastCategoryChangeResult) {
        categoryChangeResult = result
    }

    var podcasts: [PodcastInventoryRow] = []
    private(set) var lastListPodcastsLimit: Int = -1
    func setPodcasts(_ rows: [PodcastInventoryRow]) { podcasts = rows }

    func listSubscriptions(limit: Int) async -> [SubscriptionSummary] {
        lastListSubscriptionsLimit = limit
        return Array(subscriptions.prefix(limit))
    }

    func listPodcasts(limit: Int) async -> [PodcastInventoryRow] {
        lastListPodcastsLimit = limit
        return Array(podcasts.prefix(limit))
    }

    func listEpisodes(podcastID: PodcastID, limit: Int) async -> [EpisodeInventoryRow]? {
        lastListEpisodesPodcastID = podcastID
        lastListEpisodesLimit = limit
        guard let rows = episodesByPodcast[podcastID] else { return nil }
        return Array(rows.prefix(limit))
    }

    func listInProgress(limit: Int) async -> [EpisodeInventoryRow] {
        lastInProgressLimit = limit
        return Array(inProgress.prefix(limit))
    }

    func listRecentUnplayed(limit: Int) async -> [EpisodeInventoryRow] {
        lastRecentUnplayedLimit = limit
        return Array(recentUnplayed.prefix(limit))
    }

    func listCategories(limit: Int, includePodcasts: Bool) async -> [PodcastCategorySummary] {
        lastListCategoriesLimit = limit
        lastListCategoriesIncludePodcasts = includePodcasts
        return Array(categories.prefix(limit))
    }

    func changePodcastCategory(
        podcastID: PodcastID,
        category: PodcastCategoryReference
    ) async throws -> PodcastCategoryChangeResult {
        lastCategoryChangePodcastID = podcastID
        lastCategoryChangeReference = category
        return categoryChangeResult ?? PodcastCategoryChangeResult(
            podcastID: podcastID,
            title: "Mock Show",
            previousCategoryID: nil,
            previousCategoryName: nil,
            categoryID: category.id ?? "category-1",
            categoryName: category.name ?? "Mock Category",
            categorySlug: category.slug ?? "mock-category"
        )
    }
}
