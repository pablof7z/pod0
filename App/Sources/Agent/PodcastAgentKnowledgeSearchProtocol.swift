import Foundation
import Pod0Core

/// Temporary native adapter for agent discovery. Rust recall remains the
/// sole evidence/ranking owner; the later agent-kernel slice removes this
/// presentation mapping entirely.
public protocol PodcastAgentKnowledgeSearchProtocol: Sendable {
    func searchEpisodes(query: String, scope: PodcastID?, limit: Int) async throws -> [EpisodeHit]
    func queryTranscriptEvidence(
        query: String,
        scope: String?,
        limit: Int
    ) async -> RecallResultProjection
    func findSimilarEpisodes(seedEpisodeID: EpisodeID, k: Int) async throws -> [EpisodeHit]
}
