import Pod0Core
@testable import Podcastr

actor MockKnowledgeSearch: PodcastAgentKnowledgeSearchProtocol {
    private let searchResult: [EpisodeHit]
    private let transcriptProjection: RecallResultProjection
    private let similarResult: [EpisodeHit]
    private(set) var lastSearchLimit: Int = -1
    private(set) var lastSimilarK: Int = -1

    init(
        searchEpisodesResult: [EpisodeHit] = [],
        transcriptProjection: RecallResultProjection = .testNoEvidence(),
        similarResult: [EpisodeHit] = []
    ) {
        self.searchResult = searchEpisodesResult
        self.transcriptProjection = transcriptProjection
        self.similarResult = similarResult
    }

    func searchEpisodes(query: String, scope: PodcastID?, limit: Int) async throws -> [EpisodeHit] {
        lastSearchLimit = limit
        return searchResult
    }

    func queryTranscriptEvidence(
        query: String,
        scope: String?,
        limit: Int
    ) async -> RecallResultProjection {
        transcriptProjection
    }

    func findSimilarEpisodes(seedEpisodeID: EpisodeID, k: Int) async throws -> [EpisodeHit] {
        lastSimilarK = k
        return similarResult
    }
}

extension RecallResultProjection {
    static func testNoEvidence() -> RecallResultProjection {
        RecallResultProjection(
            queryId: RecallQueryId(high: 90, low: 1),
            stage: .noEvidence,
            evidence: [],
            failure: nil,
            operation: nil
        )
    }
}
