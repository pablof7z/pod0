import Foundation

extension VectorIndex {
    typealias MetadataFilter = (sql: String, params: [any Sendable], matchesNothing: Bool)

    static func metadataFilter(for scope: ChunkScope) -> MetadataFilter {
        switch scope {
        case .all:
            return ("", [], false)
        case let .podcast(id):
            return (" AND podcast_id = ?", [id.uuidString], false)
        case let .episodes(ids):
            guard !ids.isEmpty else { return ("", [], true) }
            let placeholders = Array(repeating: "?", count: ids.count).joined(separator: ",")
            return (" AND episode_id IN (\(placeholders))", ids.map(\.uuidString), false)
        case let .episode(id):
            return (" AND episode_id = ?", [id.uuidString], false)
        case let .speaker(id):
            return (" AND speaker_id = ?", [id.uuidString], false)
        case .transcripts:
            return transcriptFilter()
        case let .transcriptsForPodcast(id):
            return transcriptFilter(sql: " AND podcast_id = ?", parameter: id)
        case let .transcriptsForEpisode(id):
            return transcriptFilter(sql: " AND episode_id = ?", parameter: id)
        }
    }

    private static func transcriptFilter(sql: String = "", parameter: UUID? = nil) -> MetadataFilter {
        var params: [any Sendable] = [semanticArtifactKind, "legacy"]
        if let parameter { params.append(parameter.uuidString) }
        return (" AND artifact_kind IN (?, ?)" + sql, params, false)
    }
}
