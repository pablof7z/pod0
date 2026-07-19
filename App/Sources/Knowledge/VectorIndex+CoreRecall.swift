import Foundation
import Pod0Core

struct CoreRecallIndexSpan: Sendable, Equatable {
    let spanID: EvidenceSpanId
    let generationID: EvidenceGenerationId
    let episodeID: EpisodeId
    let podcastID: PodcastId
    let text: String
}

extension VectorIndex {
    func rebuildCoreRecallIndex(spans: [CoreRecallIndexSpan]) async throws -> UInt32 {
        try await ensureRecallSchema()
        guard let first = spans.first,
              spans.allSatisfy({
                  $0.episodeID == first.episodeID && $0.generationID == first.generationID
              }),
              Set(spans.map(\.spanID)).count == spans.count
        else {
            throw VectorStoreError.backingStorageFailure("Invalid recall index generation")
        }
        if try await coreRecallIndexMatches(spans) {
            return try boundedRecallCount(spans.count)
        }

        let vectors = try await embedder.embed(spans.map(\.text))
        guard vectors.count == spans.count else {
            throw VectorStoreError.backingStorageFailure("Recall embedding count mismatch")
        }
        for vector in vectors where vector.count != dimensions {
            throw VectorStoreError.dimensionMismatch(expected: dimensions, got: vector.count)
        }

        let episodeKey = Self.coreKey(first.episodeID)
        _ = try await db.execute("BEGIN TRANSACTION")
        do {
            let rows = try await db.query(
                "SELECT span_id FROM core_recall_meta_v1 WHERE episode_id=?",
                params: [episodeKey]
            )
            for spanKey in rows.compactMap({ $0["span_id"] as? String }) {
                _ = try await db.execute(
                    "DELETE FROM core_recall_vec_v1 WHERE span_id=?", params: [spanKey]
                )
                _ = try await db.execute(
                    "DELETE FROM core_recall_fts_v1 WHERE span_id=?", params: [spanKey]
                )
            }
            _ = try await db.execute(
                "DELETE FROM core_recall_meta_v1 WHERE episode_id=?", params: [episodeKey]
            )
            for (span, vector) in zip(spans, vectors) {
                try await insertCoreRecallSpan(span, vector: vector)
            }
            _ = try await db.execute("COMMIT TRANSACTION")
        } catch {
            _ = try? await db.execute("ROLLBACK TRANSACTION")
            throw error
        }
        guard try await coreRecallIndexMatches(spans) else {
            throw VectorStoreError.backingStorageFailure("Recall index verification failed")
        }
        return try boundedRecallCount(spans.count)
    }

    func ensureRecallSchema() async throws {
        _ = try await db.execute(
            """
            CREATE TABLE IF NOT EXISTS core_recall_meta_v1(
                span_id TEXT PRIMARY KEY,
                generation_id TEXT NOT NULL,
                episode_id TEXT NOT NULL,
                podcast_id TEXT NOT NULL,
                text TEXT NOT NULL
            )
            """
        )
        _ = try await db.execute(
            "CREATE INDEX IF NOT EXISTS idx_core_recall_episode_v1 ON core_recall_meta_v1(episode_id)"
        )
        _ = try await db.execute(
            "CREATE INDEX IF NOT EXISTS idx_core_recall_podcast_v1 ON core_recall_meta_v1(podcast_id)"
        )
        _ = try await db.execute(
            """
            CREATE VIRTUAL TABLE IF NOT EXISTS core_recall_vec_v1 USING vec0(
                span_id TEXT PRIMARY KEY,
                episode_id TEXT PARTITION KEY,
                podcast_id TEXT PARTITION KEY,
                embedding FLOAT[\(dimensions)] distance_metric=cosine
            )
            """
        )
        _ = try await db.execute(
            """
            CREATE VIRTUAL TABLE IF NOT EXISTS core_recall_fts_v1 USING fts5(
                span_id UNINDEXED,
                episode_id UNINDEXED,
                podcast_id UNINDEXED,
                text,
                tokenize='porter'
            )
            """
        )
    }

    private func insertCoreRecallSpan(
        _ span: CoreRecallIndexSpan,
        vector: [Float]
    ) async throws {
        let spanKey = Self.coreKey(span.spanID)
        let generationKey = Self.coreKey(span.generationID)
        let episodeKey = Self.coreKey(span.episodeID)
        let podcastKey = Self.coreKey(span.podcastID)
        _ = try await db.execute(
            """
            INSERT INTO core_recall_meta_v1(span_id,generation_id,episode_id,podcast_id,text)
            VALUES (?,?,?,?,?)
            """,
            params: [spanKey, generationKey, episodeKey, podcastKey, span.text]
        )
        _ = try await db.execute(
            """
            INSERT INTO core_recall_vec_v1(span_id,episode_id,podcast_id,embedding)
            VALUES (?,?,?,?)
            """,
            params: [spanKey, episodeKey, podcastKey, vector]
        )
        _ = try await db.execute(
            """
            INSERT INTO core_recall_fts_v1(span_id,episode_id,podcast_id,text)
            VALUES (?,?,?,?)
            """,
            params: [spanKey, episodeKey, podcastKey, span.text]
        )
    }

    private func coreRecallIndexMatches(_ spans: [CoreRecallIndexSpan]) async throws -> Bool {
        guard let first = spans.first else { return false }
        let rows = try await db.query(
            """
            SELECT m.span_id,m.generation_id,
              CASE WHEN v.span_id IS NULL THEN 0 ELSE 1 END AS has_vector,
              CASE WHEN f.span_id IS NULL THEN 0 ELSE 1 END AS has_lexical
            FROM core_recall_meta_v1 m
            LEFT JOIN core_recall_vec_v1 v ON v.span_id=m.span_id
            LEFT JOIN core_recall_fts_v1 f ON f.span_id=m.span_id
            WHERE m.episode_id=?
            """,
            params: [Self.coreKey(first.episodeID)]
        )
        guard rows.count == spans.count else { return false }
        let expected = Set(spans.map { Self.coreKey($0.spanID) })
        let generation = Self.coreKey(first.generationID)
        return Set(rows.compactMap { $0["span_id"] as? String }) == expected
            && rows.allSatisfy {
                ($0["generation_id"] as? String) == generation
                    && ($0["has_vector"] as? Int) == 1
                    && ($0["has_lexical"] as? Int) == 1
            }
    }

    private func boundedRecallCount(_ count: Int) throws -> UInt32 {
        guard let value = UInt32(exactly: count) else {
            throw VectorStoreError.backingStorageFailure("Recall span count exceeds UInt32")
        }
        return value
    }
}

private extension VectorIndex {
    static func coreKey(_ value: EvidenceSpanId) -> String { coreKey(value.high, value.low) }
    static func coreKey(_ value: EvidenceGenerationId) -> String { coreKey(value.high, value.low) }
    static func coreKey(_ value: EpisodeId) -> String { coreKey(value.high, value.low) }
    static func coreKey(_ value: PodcastId) -> String { coreKey(value.high, value.low) }

    static func coreKey(_ high: UInt64, _ low: UInt64) -> String {
        String(format: "%016llx%016llx", high, low)
    }
}
