import CryptoKit
import Foundation

struct VectorArtifactReceipt: Codable, Sendable, Equatable {
    let generation: String
    let artifactKind: String
    let chunkCount: Int
    let schemaVersion: Int
}

extension VectorIndex {
    static let semanticArtifactKind = "semantic-transcript"
    static let metadataArtifactKind = "episode-metadata"
    static let artifactSchemaVersion = 2

    func stageArtifact(
        chunks: [Chunk],
        episodeID: UUID,
        generation: String,
        artifactKind: String
    ) async throws -> VectorArtifactReceipt {
        try await ensureSchema()
        let vectors = chunks.isEmpty ? [] : try await embedder.embed(chunks.map(\.text))
        guard vectors.count == chunks.count else {
            throw VectorStoreError.backingStorageFailure(
                "Embedder returned \(vectors.count) vectors for \(chunks.count) chunks"
            )
        }
        for vector in vectors where vector.count != dimensions {
            throw VectorStoreError.dimensionMismatch(expected: dimensions, got: vector.count)
        }
        let versioned = chunks.map { chunk -> Chunk in
            var copy = chunk
            copy.id = Self.versionedID(base: chunk.id, generation: generation, kind: artifactKind)
            return copy
        }
        _ = try await db.execute("BEGIN TRANSACTION")
        do {
            try await deleteArtifactRows(
                episodeID: episodeID, generation: generation, artifactKind: artifactKind
            )
            for (chunk, vector) in zip(versioned, vectors) {
                try await upsertOne(
                    chunk: chunk,
                    vector: vector,
                    generation: generation,
                    artifactKind: artifactKind,
                    selected: false
                )
            }
            _ = try await db.execute("COMMIT TRANSACTION")
        } catch {
            _ = try? await db.execute("ROLLBACK TRANSACTION")
            throw error
        }
        return VectorArtifactReceipt(
            generation: generation,
            artifactKind: artifactKind,
            chunkCount: versioned.count,
            schemaVersion: Self.artifactSchemaVersion
        )
    }

    func verifyArtifact(
        episodeID: UUID,
        receipt: VectorArtifactReceipt
    ) async throws -> Bool {
        try await ensureSchema()
        let rows = try await db.query(
            """
            SELECT COUNT(*) AS meta_count,
              SUM(CASE WHEN v.chunk_id IS NOT NULL THEN 1 ELSE 0 END) AS vec_count,
              SUM(CASE WHEN f.chunk_id IS NOT NULL THEN 1 ELSE 0 END) AS fts_count
            FROM chunks_meta m
            LEFT JOIN chunks_vec v ON v.chunk_id=m.chunk_id
            LEFT JOIN chunks_fts f ON f.chunk_id=m.chunk_id
            WHERE m.episode_id=? AND m.generation=? AND m.artifact_kind=?
            """,
            params: [episodeID.uuidString, receipt.generation, receipt.artifactKind]
        )
        guard let row = rows.first else { return false }
        let metadata = (row["meta_count"] as? Int) ?? 0
        let vectors = (row["vec_count"] as? Int) ?? 0
        let fts = (row["fts_count"] as? Int) ?? 0
        return receipt.schemaVersion == Self.artifactSchemaVersion
            && metadata == receipt.chunkCount
            && vectors == receipt.chunkCount
            && fts == receipt.chunkCount
    }

    func selectArtifact(
        episodeID: UUID,
        receipt: VectorArtifactReceipt
    ) async throws {
        guard try await verifyArtifact(episodeID: episodeID, receipt: receipt) else {
            throw VectorStoreError.backingStorageFailure("Vector artifact verification failed")
        }
        _ = try await db.execute("BEGIN TRANSACTION")
        do {
            _ = try await db.execute(
                "UPDATE chunks_meta SET selected=0 WHERE episode_id=? AND artifact_kind=?",
                params: [episodeID.uuidString, receipt.artifactKind]
            )
            _ = try await db.execute(
                """
                UPDATE chunks_meta SET selected=1
                WHERE episode_id=? AND generation=? AND artifact_kind=?
                """,
                params: [episodeID.uuidString, receipt.generation, receipt.artifactKind]
            )
            _ = try await db.execute("COMMIT TRANSACTION")
        } catch {
            _ = try? await db.execute("ROLLBACK TRANSACTION")
            throw error
        }
    }

    func selectedReceipt(
        episodeID: UUID,
        artifactKind: String
    ) async throws -> VectorArtifactReceipt? {
        try await ensureSchema()
        let rows = try await db.query(
            """
            SELECT generation,COUNT(*) AS count FROM chunks_meta
            WHERE episode_id=? AND artifact_kind=? AND selected=1
            GROUP BY generation
            """,
            params: [episodeID.uuidString, artifactKind]
        )
        guard rows.count == 1,
              let generation = rows[0]["generation"] as? String,
              let count = rows[0]["count"] as? Int else { return nil }
        return VectorArtifactReceipt(
            generation: generation,
            artifactKind: artifactKind,
            chunkCount: count,
            schemaVersion: Self.artifactSchemaVersion
        )
    }

    private func deleteArtifactRows(
        episodeID: UUID,
        generation: String,
        artifactKind: String
    ) async throws {
        let rows = try await db.query(
            """
            SELECT chunk_id FROM chunks_meta
            WHERE episode_id=? AND generation=? AND artifact_kind=?
            """,
            params: [episodeID.uuidString, generation, artifactKind]
        )
        for chunkID in rows.compactMap({ $0["chunk_id"] as? String }) {
            _ = try await db.execute("DELETE FROM chunks_vec WHERE chunk_id=?", params: [chunkID])
            _ = try await db.execute("DELETE FROM chunks_fts WHERE chunk_id=?", params: [chunkID])
        }
        _ = try await db.execute(
            "DELETE FROM chunks_meta WHERE episode_id=? AND generation=? AND artifact_kind=?",
            params: [episodeID.uuidString, generation, artifactKind]
        )
    }

    private static func versionedID(base: UUID, generation: String, kind: String) -> UUID {
        let digest = Array(SHA256.hash(data: Data("\(base.uuidString):\(generation):\(kind)".utf8)))
        return UUID(uuid: (
            digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
            digest[8], digest[9], digest[10], digest[11], digest[12], digest[13], digest[14], digest[15]
        ))
    }

    static func rrf(
        vecRanks: [String],
        ftsRanks: [String],
        k: Double = 60
    ) -> [(cid: String, score: Float)] {
        var scores: [String: Double] = [:]
        for (index, id) in vecRanks.enumerated() {
            scores[id, default: 0] += 1.0 / (k + Double(index + 1))
        }
        for (index, id) in ftsRanks.enumerated() {
            scores[id, default: 0] += 1.0 / (k + Double(index + 1))
        }
        return scores.sorted { $0.value > $1.value }
            .map { (cid: $0.key, score: Float($0.value)) }
    }

    static func sanitizeFTSQuery(_ raw: String) -> String {
        let cleaned = raw.unicodeScalars.map { scalar -> Character in
            CharacterSet.alphanumerics.contains(scalar) || scalar == " "
                ? Character(scalar) : " "
        }
        return String(cleaned).trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
