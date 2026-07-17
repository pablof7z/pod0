import XCTest
@testable import Podcastr

/// Tests for search, transcript, perplexity, summarize, and find-similar
/// dispatch paths. Playback and action tool tests live in
/// `AgentToolsPodcastTests.swift`.
@MainActor
final class AgentToolsPodcastSearchTests: XCTestCase {

    // MARK: - search_episodes

    func testSearchEpisodesReturnsRows() async throws {
        let hits = [
            EpisodeHit(episodeID: "ep1", podcastID: "pod1", title: "Zone 2 Conversation", podcastTitle: "Tim Ferriss", score: 0.91),
            EpisodeHit(episodeID: "ep2", podcastID: "pod2", title: "VO2 Max", podcastTitle: "Huberman", score: 0.78),
        ]
        let deps = makeDeps(rag: MockRAG(searchEpisodesResult: hits))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.searchEpisodes,
            args: ["query": "zone 2", "limit": 5],
            deps: deps
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["success"] as? Bool, true)
        XCTAssertEqual(decoded["total_found"] as? Int, 2)
        let rows = decoded["results"] as? [[String: Any]]
        XCTAssertEqual(rows?.count, 2)
        XCTAssertEqual(rows?.first?["episode_id"] as? String, "ep1")
        XCTAssertEqual(rows?.first?["score"] as? Double, 0.91)
    }

    func testSearchEpisodesClampsLimitAboveMax() async throws {
        let mockRAG = MockRAG()
        let deps = makeDeps(rag: mockRAG)
        _ = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.searchEpisodes,
            args: ["query": "anything", "limit": 9_999],
            deps: deps
        )
        let lastLimit = await mockRAG.lastSearchLimit
        XCTAssertEqual(lastLimit, AgentTools.podcastSearchMaxLimit)
    }

    func testSearchEpisodesRequiresQuery() async throws {
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.searchEpisodes,
            args: ["query": "  "],
            deps: makeDeps()
        )
        XCTAssertNotNil(try decode(json)["error"])
    }

    // MARK: - query_transcripts

    func testQueryTranscriptsReturnsChunksWithTimestamps() async throws {
        let deps = makeDeps(rag: MockRAG(transcriptsResult: [
            TranscriptHit(episodeID: "ep1", startSeconds: 47.0, endSeconds: 60.0, speaker: "Tim", text: "Zone 2 is sustained..."),
        ]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.queryTranscripts,
            args: ["query": "zone 2", "scope": "ep1"],
            deps: deps
        )
        let decoded = try decode(json)
        let rows = decoded["results"] as? [[String: Any]]
        XCTAssertEqual(rows?.count, 1)
        XCTAssertEqual(rows?.first?["speaker"] as? String, "Tim")
        XCTAssertEqual(rows?.first?["start_seconds"] as? Double, 47.0)
    }

    // MARK: - perplexity_search

    func testPerplexitySearchPropagatesAnswerAndSources() async throws {
        let deps = makeDeps(perplexity: MockPerplexity(result: PerplexityResult(
            answer: "It rained.",
            sources: [.init(title: "weather.com", url: "https://weather.com/x")]
        )))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.perplexitySearch,
            args: ["query": "did it rain in Tokyo yesterday?"],
            deps: deps
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["answer"] as? String, "It rained.")
        let sources = decoded["sources"] as? [[String: Any]]
        XCTAssertEqual(sources?.first?["url"] as? String, "https://weather.com/x")
    }

    func testPerplexitySearchSurfacesError() async throws {
        let deps = makeDeps(perplexity: MockPerplexity(error: PerplexityClientError.missingAPIKey))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.perplexitySearch,
            args: ["query": "anything"],
            deps: deps
        )
        XCTAssertNotNil(try decode(json)["error"])
    }

    // MARK: - summarize_episode

    func testSummarizeEpisodeSuccess() async throws {
        let deps = makeDeps(
            summarizer: MockSummarizer(result: EpisodeSummary(
                episodeID: "ep1", summary: "Quick TLDR.", bulletPoints: ["A", "B"]
            )),
            fetcher: MockFetcher(known: ["ep1"])
        )
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.summarizeEpisode,
            args: ["episode_id": "ep1", "length": "short"],
            deps: deps
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["summary"] as? String, "Quick TLDR.")
        XCTAssertEqual(decoded["length"] as? String, "short")
        XCTAssertEqual((decoded["bullets"] as? [String])?.count, 2)
    }

    // MARK: - find_similar_episodes

    func testFindSimilarEpisodesUsesK() async throws {
        let mockRAG = MockRAG(similarResult: [
            EpisodeHit(episodeID: "ep2", podcastID: "pod1", title: "Sequel", podcastTitle: "Tim Ferriss"),
        ])
        let deps = makeDeps(rag: mockRAG, fetcher: MockFetcher(known: ["seed"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.findSimilarEpisodes,
            args: ["seed_episode_id": "seed", "k": 7],
            deps: deps
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["k"] as? Int, 7)
        let kSeen = await mockRAG.lastSimilarK
        XCTAssertEqual(kSeen, 7)
    }

    func testFindSimilarEpisodesRejectsUnknownSeed() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: []))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.findSimilarEpisodes,
            args: ["seed_episode_id": "ghost"],
            deps: deps
        )
        XCTAssertNotNil(try decode(json)["error"])
    }

    // MARK: - Helpers

    private func decode(_ json: String) throws -> [String: Any] {
        let raw = try JSONSerialization.jsonObject(with: Data(json.utf8))
        guard let obj = raw as? [String: Any] else {
            throw NSError(domain: "test", code: 1, userInfo: [NSLocalizedDescriptionKey: "non-object JSON"])
        }
        return obj
    }

    private func makeDeps(
        rag: PodcastAgentRAGSearchProtocol = MockRAG(),
        summarizer: EpisodeSummarizerProtocol = MockSummarizer(),
        fetcher: EpisodeFetcherProtocol = MockFetcher(),
        perplexity: PerplexityClientProtocol = MockPerplexity()
    ) -> PodcastAgentToolDeps {
        PodcastAgentToolDeps(
            rag: rag,
            summarizer: summarizer,
            fetcher: fetcher,
            playback: MockPlayback(),
            library: MockLibrary(),
            inventory: MockInventory(),
            categories: MockInventory(),
            peerPublisher: MockPeerEventPublisher(),
            friendDirectory: MockFriendDirectory(),
            perplexity: perplexity,
            ttsPublisher: MockTTSPublisher(),
            directory: MockDirectory(),
            subscribe: MockSubscribe(),
            youtubeIngestion: MockYouTubeIngestion(),
            ownedPodcasts: MockOwnedPodcasts()
        )
    }
}
