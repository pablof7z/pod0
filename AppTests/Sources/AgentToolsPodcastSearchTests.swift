import Pod0Core
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
        let deps = makeDeps(knowledge: MockKnowledgeSearch(searchEpisodesResult: hits))
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
        let mockRAG = MockKnowledgeSearch()
        let deps = makeDeps(knowledge: mockRAG)
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
        let evidence = recallEvidence()
        let projection = RecallResultProjection(
            queryId: RecallQueryId(high: 90, low: 2),
            stage: .ready,
            evidence: [evidence],
            failure: nil,
            operation: nil
        )
        let deps = makeDeps(knowledge: MockKnowledgeSearch(transcriptProjection: projection))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.queryTranscripts,
            args: ["query": "zone 2", "scope": "ep1"],
            deps: deps
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["status"] as? String, "ready")
        let rows = decoded["results"] as? [[String: Any]]
        XCTAssertEqual(rows?.count, 1)
        XCTAssertEqual(rows?.first?["start_seconds"] as? Double, 47.0)
        XCTAssertEqual(rows?.first?["span_id"] as? String, evidence.spanId.stableString)
        XCTAssertEqual(rows?.first?["generation_id"] as? String, evidence.generationId.stableString)
        XCTAssertEqual(rows?.first?["podcast_id"] as? String, evidence.podcastId.uuid?.uuidString)
        let provenance = rows?.first?["provenance"] as? [String: Any]
        XCTAssertEqual(provenance?["source"] as? String, "publisher")
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
        let mockRAG = MockKnowledgeSearch(similarResult: [
            EpisodeHit(episodeID: "ep2", podcastID: "pod1", title: "Sequel", podcastTitle: "Tim Ferriss"),
        ])
        let deps = makeDeps(knowledge: mockRAG, fetcher: MockFetcher(known: ["seed"]))
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
        knowledge: PodcastAgentKnowledgeSearchProtocol = MockKnowledgeSearch(),
        summarizer: EpisodeSummarizerProtocol = MockSummarizer(),
        fetcher: EpisodeFetcherProtocol = MockFetcher(),
        perplexity: PerplexityClientProtocol = MockPerplexity()
    ) -> PodcastAgentToolDeps {
        PodcastAgentToolDeps(
            knowledge: knowledge,
            summarizer: summarizer,
            fetcher: fetcher,
            playback: MockPlayback(),
            library: MockLibrary(),
            inventory: MockInventory(),
            categories: MockInventory(),
            perplexity: perplexity,
            ttsPublisher: MockTTSPublisher(),
            directory: MockDirectory(),
            subscribe: MockSubscribe(),
            youtubeIngestion: MockYouTubeIngestion(),
            ownedPodcasts: MockOwnedPodcasts()
        )
    }

    private func recallEvidence() -> RecallEvidenceProjection {
        RecallEvidenceProjection(
            episodeId: EpisodeId(uuid: UUID(uuidString: "11111111-1111-1111-1111-111111111111")!),
            podcastId: PodcastId(uuid: UUID(uuidString: "22222222-2222-2222-2222-222222222222")!),
            generationId: EvidenceGenerationId(high: 1, low: 2),
            transcriptVersionId: TranscriptVersionId(high: 3, low: 4),
            transcriptContentDigest: ContentDigest(word0: 5, word1: 6, word2: 7, word3: 8),
            spanId: EvidenceSpanId(high: 9, low: 10),
            firstSegmentId: TranscriptSegmentId(high: 11, low: 12),
            lastSegmentId: TranscriptSegmentId(high: 13, low: 14),
            startSegmentOrdinal: 2,
            endSegmentOrdinalExclusive: 4,
            startMilliseconds: 47_000,
            endMilliseconds: 60_000,
            excerpt: "Zone 2 is sustained...",
            speakerId: SpeakerId(high: 15, low: 16),
            provenance: Pod0Core.TranscriptProvenance(
                source: .publisher,
                provider: "fixture",
                sourcePayloadDigest: ContentDigest(word0: 17, word1: 18, word2: 19, word3: 20)
            ),
            score: RecallScoreProjection(
                vectorRrfUnits: 10,
                lexicalRrfUnits: 11,
                totalRrfUnits: 21,
                baseRank: 1,
                rerankRank: nil
            )
        )
    }
}
