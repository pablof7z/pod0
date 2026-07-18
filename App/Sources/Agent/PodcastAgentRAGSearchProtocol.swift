import Foundation

/// Typed retrieval seam used by the temporary Swift recall proof and agent
/// tools. Issue #69 replaces its implementation with the shared Rust owner.
public protocol PodcastAgentRAGSearchProtocol: Sendable {
    func searchEpisodes(query: String, scope: PodcastID?, limit: Int) async throws -> [EpisodeHit]
    func queryTranscripts(query: String, scope: String?, limit: Int) async throws -> [TranscriptHit]
    func transcriptCorpusReadiness() async -> TranscriptCorpusReadiness
    func findSimilarEpisodes(seedEpisodeID: EpisodeID, k: Int) async throws -> [EpisodeHit]
}

public extension PodcastAgentRAGSearchProtocol {
    func transcriptCorpusReadiness() async -> TranscriptCorpusReadiness { .unavailable }
}
