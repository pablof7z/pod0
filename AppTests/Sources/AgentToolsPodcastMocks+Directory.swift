import Foundation
@testable import Podcastr

// MARK: - Directory + Subscribe + OwnedPodcasts + YouTube mocks
//
// Pulled out of `AgentToolsPodcastMocks.swift` to keep that file under the
// 500-line hard limit set by `AGENTS.md`. These mocks back the iTunes
// directory lookup, `subscribe_podcast`, agent-owned podcast management,
// and YouTube ingestion tool paths.

actor MockDirectory: PodcastDirectoryProtocol {
    /// Map of collection_id → feed_url for `lookupFeedURL`. nil ⇒ "not found".
    private var feedURLByCollectionID: [String: String] = [:]
    private var lookupError: Error?
    private(set) var lookupCalls: [String] = []

    init(feedURLByCollectionID: [String: String] = [:], lookupError: Error? = nil) {
        self.feedURLByCollectionID = feedURLByCollectionID
        self.lookupError = lookupError
    }

    func setFeedURL(_ feedURL: String?, forCollectionID id: String) {
        if let feedURL { feedURLByCollectionID[id] = feedURL }
        else { feedURLByCollectionID.removeValue(forKey: id) }
    }

    func setLookupError(_ error: Error?) {
        lookupError = error
    }

    func searchDirectory(
        query: String,
        type: PodcastDirectorySearchType,
        limit: Int
    ) async throws -> [PodcastDirectoryHit] {
        return []
    }

    func lookupFeedURL(forCollectionID collectionID: String) async throws -> String? {
        lookupCalls.append(collectionID)
        if let lookupError { throw lookupError }
        return feedURLByCollectionID[collectionID]
    }
}

actor MockSubscribe: PodcastSubscribeProtocol {
    /// Per-feed-URL canned response for `ensurePodcast`. Used by the
    /// `list_episodes` external-input tests so the test author can pin the
    /// resolved podcast_id to a value the inventory mock has episodes for.
    private var ensureResults: [String: PodcastEnsureResult] = [:]
    private var ensureError: Error?
    private(set) var ensureCalls: [String] = []

    /// Per-feed-URL canned response for `subscribe`.
    private var subscribeResults: [String: PodcastSubscribeResult] = [:]
    private(set) var subscribeCalls: [String] = []

    init(
        ensureResults: [String: PodcastEnsureResult] = [:],
        ensureError: Error? = nil
    ) {
        self.ensureResults = ensureResults
        self.ensureError = ensureError
    }

    func setEnsureResult(_ result: PodcastEnsureResult, forFeedURL feedURL: String) {
        ensureResults[feedURL] = result
    }

    func setEnsureError(_ error: Error?) {
        ensureError = error
    }

    func subscribe(feedURLString: String) async throws -> PodcastSubscribeResult {
        subscribeCalls.append(feedURLString)
        if let canned = subscribeResults[feedURLString] {
            return canned
        }
        return PodcastSubscribeResult(
            podcastID: "mock-pod",
            title: "Mock Show",
            feedURL: feedURLString,
            episodeCount: 0,
            alreadySubscribed: false
        )
    }

    func ensurePodcast(feedURLString: String) async throws -> PodcastEnsureResult {
        ensureCalls.append(feedURLString)
        if let ensureError { throw ensureError }
        if let canned = ensureResults[feedURLString] {
            return canned
        }
        return PodcastEnsureResult(
            podcastID: "mock-ensured-pod",
            title: "Mock Ensured Show",
            feedURL: feedURLString,
            episodeCount: 0
        )
    }

    private(set) var deleteCalls: [PodcastID] = []
    private var deleteResults: [PodcastID: PodcastDeleteResult] = [:]
    private var deleteError: Error?

    func setDeleteResult(_ result: PodcastDeleteResult, forPodcastID podcastID: PodcastID) {
        deleteResults[podcastID] = result
    }

    func setDeleteError(_ error: Error?) {
        deleteError = error
    }

    func deletePodcast(podcastID: PodcastID) async throws -> PodcastDeleteResult {
        deleteCalls.append(podcastID)
        if let deleteError { throw deleteError }
        if let canned = deleteResults[podcastID] { return canned }
        return PodcastDeleteResult(
            podcastID: podcastID,
            title: "Mock Show",
            wasSubscribed: true,
            episodesDeleted: 0
        )
    }
}

// MARK: - MockOwnedPodcasts

actor MockOwnedPodcasts: AgentOwnedPodcastManagerProtocol {
    enum PublishError: Error { case notOwned, privateVisibility, nostrDisabled }

    private(set) var publishedEpisodeIDs: [EpisodeID] = []
    private var shouldFailPublish: Bool = false
    private var publishError: Error?
    var ownedPodcasts: [AgentOwnedPodcastInfo] = []

    func setShouldFailPublish(_ value: Bool) { shouldFailPublish = value }
    func setPublishError(_ error: Error?) { publishError = error }

    func createPodcast(
        title: String, description: String, author: String,
        imageURL: URL?, language: String?, categories: [String],
        visibility: Podcast.NostrVisibility
    ) async throws -> AgentOwnedPodcastInfo {
        let info = AgentOwnedPodcastInfo(
            podcastID: UUID().uuidString, title: title, description: description,
            author: author, imageURL: imageURL, visibility: visibility.rawValue,
            episodeCount: 0, nostrEventID: nil, nostrAddr: nil, episodesPublishedToNostr: nil
        )
        ownedPodcasts.append(info)
        return info
    }

    func updatePodcast(
        podcastID: PodcastID, title: String?, description: String?,
        author: String?, imageURL: URL?, visibility: Podcast.NostrVisibility?
    ) async throws -> AgentOwnedPodcastInfo {
        guard let idx = ownedPodcasts.firstIndex(where: { $0.podcastID == podcastID }) else {
            throw AgentOwnedPodcastError.notFound(podcastID)
        }
        let existing = ownedPodcasts[idx]
        let updated = AgentOwnedPodcastInfo(
            podcastID: podcastID,
            title: title ?? existing.title,
            description: description ?? existing.description,
            author: author ?? existing.author,
            imageURL: imageURL ?? existing.imageURL,
            visibility: visibility?.rawValue ?? existing.visibility,
            episodeCount: existing.episodeCount,
            nostrEventID: nil, nostrAddr: nil, episodesPublishedToNostr: nil
        )
        ownedPodcasts[idx] = updated
        return updated
    }

    func deletePodcast(podcastID: PodcastID) async throws {
        ownedPodcasts.removeAll { $0.podcastID == podcastID }
    }

    func listOwnedPodcasts() async -> [AgentOwnedPodcastInfo] { ownedPodcasts }

    func generateAndUploadArtwork(prompt: String) async throws -> URL {
        return URL(string: "https://blossom.example.com/mock-artwork.png")!
    }

    func publishEpisodeToNostr(episodeID: EpisodeID) async throws -> String? {
        if let err = publishError { throw err }
        if shouldFailPublish { return nil }
        publishedEpisodeIDs.append(episodeID)
        return "naddr1mock\(episodeID)"
    }
}

// MARK: - MockYouTubeIngestion

actor MockYouTubeIngestion: YouTubeIngestionProtocol {
    func ingestVideo(
        youtubeURL: String, customTitle: String?, transcribe: Bool
    ) async throws -> YouTubeIngestionResult {
        return YouTubeIngestionResult(
            episodeID: "mock-yt-episode",
            title: customTitle ?? "Mock YouTube Video",
            author: "Mock Channel",
            durationSeconds: 600,
            transcriptStatus: nil
        )
    }

    func searchVideos(query: String, limit: Int) async throws -> [YouTubeSearchResult] {
        return []
    }
}
