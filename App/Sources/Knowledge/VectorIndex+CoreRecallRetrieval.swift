import Foundation
import Pod0Core

extension VectorIndex {
    func retrieveCoreRecallCandidates(
        queryVector: [Float],
        lexicalQuery: String,
        scope: RecallScope,
        maximumVectorCandidates: UInt16,
        maximumLexicalCandidates: UInt16,
        maximumTotalCandidates: UInt16
    ) async throws -> [RecallCandidateObservation] {
        try await ensureRecallSchema()
        guard queryVector.count == dimensions else {
            throw VectorStoreError.dimensionMismatch(expected: dimensions, got: queryVector.count)
        }
        guard Int(maximumVectorCandidates) + Int(maximumLexicalCandidates)
                <= Int(maximumTotalCandidates) else {
            throw VectorStoreError.backingStorageFailure("Invalid recall candidate limits")
        }
        let filter = try Self.coreRecallFilter(scope)
        let vectorRows = try await coreVectorRows(
            queryVector: queryVector,
            filter: filter,
            limit: Int(maximumVectorCandidates)
        )
        let lexicalRows = try await coreLexicalRows(
            query: lexicalQuery,
            filter: filter,
            limit: Int(maximumLexicalCandidates)
        )
        var ranks: [String: (vector: UInt16?, lexical: UInt16?)] = [:]
        for (index, row) in vectorRows.enumerated() {
            guard let key = row["span_id"] as? String,
                  let rank = UInt16(exactly: index + 1) else { continue }
            ranks[key] = (rank, ranks[key]?.lexical)
        }
        for (index, row) in lexicalRows.enumerated() {
            guard let key = row["span_id"] as? String,
                  let rank = UInt16(exactly: index + 1) else { continue }
            ranks[key] = (ranks[key]?.vector, rank)
        }
        guard ranks.count <= Int(maximumTotalCandidates) else {
            throw VectorStoreError.backingStorageFailure("Recall candidate union exceeds limit")
        }
        let metadata = try await coreRecallMetadata(keys: ranks.keys.sorted())
        guard metadata.count == ranks.count else {
            throw VectorStoreError.backingStorageFailure("Recall metadata is incomplete")
        }
        return try ranks.keys.sorted().map { key in
            guard let row = metadata[key], let rank = ranks[key],
                  let episode = Self.episodeID(row["episode_id"] as? String),
                  let generation = Self.generationID(row["generation_id"] as? String),
                  let span = Self.spanID(key)
            else {
                throw VectorStoreError.backingStorageFailure("Recall identifier is malformed")
            }
            return RecallCandidateObservation(
                episodeId: episode,
                generationId: generation,
                spanId: span,
                vectorRank: rank.vector,
                lexicalRank: rank.lexical
            )
        }
    }

    private func coreVectorRows(
        queryVector: [Float],
        filter: CoreRecallFilter,
        limit: Int
    ) async throws -> [[String: any Sendable]] {
        guard limit > 0 else { return [] }
        return try await db.query(
            """
            SELECT span_id,distance FROM core_recall_vec_v1
            WHERE embedding MATCH ?\(filter.vectorSQL)
            ORDER BY distance LIMIT ?
            """,
            params: [queryVector] + filter.params + [limit]
        )
    }

    private func coreLexicalRows(
        query: String,
        filter: CoreRecallFilter,
        limit: Int
    ) async throws -> [[String: any Sendable]] {
        guard limit > 0 else { return [] }
        let expression = Self.coreFTSExpression(query)
        guard !expression.isEmpty else { return [] }
        return try await db.query(
            """
            SELECT span_id,bm25(core_recall_fts_v1) AS score
            FROM core_recall_fts_v1
            WHERE core_recall_fts_v1 MATCH ?\(filter.lexicalSQL)
            ORDER BY score,span_id LIMIT ?
            """,
            params: [expression] + filter.params + [limit]
        )
    }

    private func coreRecallMetadata(
        keys: [String]
    ) async throws -> [String: [String: any Sendable]] {
        guard !keys.isEmpty else { return [:] }
        let placeholders = Array(repeating: "?", count: keys.count).joined(separator: ",")
        let rows = try await db.query(
            """
            SELECT span_id,generation_id,episode_id
            FROM core_recall_meta_v1 WHERE span_id IN (\(placeholders))
            """,
            params: keys
        )
        return Dictionary(uniqueKeysWithValues: rows.compactMap { row in
            (row["span_id"] as? String).map { ($0, row) }
        })
    }
}

private extension VectorIndex {
    struct CoreRecallFilter {
        let vectorSQL: String
        let lexicalSQL: String
        let params: [any Sendable]
    }

    static func coreRecallFilter(_ scope: RecallScope) throws -> CoreRecallFilter {
        switch scope {
        case .library:
            CoreRecallFilter(vectorSQL: "", lexicalSQL: "", params: [])
        case .podcast(let id):
            CoreRecallFilter(
                vectorSQL: " AND podcast_id=?",
                lexicalSQL: " AND podcast_id=?",
                params: [coreRecallKey(id.high, id.low)]
            )
        case .episode(let id):
            CoreRecallFilter(
                vectorSQL: " AND episode_id=?",
                lexicalSQL: " AND episode_id=?",
                params: [coreRecallKey(id.high, id.low)]
            )
        case .unsupported:
            throw VectorStoreError.backingStorageFailure("Unsupported recall scope")
        }
    }

    static func coreFTSExpression(_ query: String) -> String {
        sanitizeFTSQuery(query)
            .split(whereSeparator: \.isWhitespace)
            .map { "\"\($0)\"" }
            .joined(separator: " AND ")
    }

    static func episodeID(_ key: String?) -> EpisodeId? {
        coreRecallParts(key).map { EpisodeId(high: $0.high, low: $0.low) }
    }

    static func generationID(_ key: String?) -> EvidenceGenerationId? {
        coreRecallParts(key).map { EvidenceGenerationId(high: $0.high, low: $0.low) }
    }

    static func spanID(_ key: String?) -> EvidenceSpanId? {
        coreRecallParts(key).map { EvidenceSpanId(high: $0.high, low: $0.low) }
    }

    static func coreRecallKey(_ high: UInt64, _ low: UInt64) -> String {
        String(format: "%016llx%016llx", high, low)
    }

    static func coreRecallParts(_ key: String?) -> (high: UInt64, low: UInt64)? {
        guard let key, key.count == 32,
              let high = UInt64(key.prefix(16), radix: 16),
              let low = UInt64(key.suffix(16), radix: 16) else { return nil }
        return (high, low)
    }
}
