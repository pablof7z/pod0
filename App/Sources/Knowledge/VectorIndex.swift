import Foundation
import os.log
import SQLiteVec

// Lane 6 — RAG: on-device vector + hybrid lexical store.
//
// One SQLite file at:
//   $applicationSupport/podcastr/vectors.sqlite
//
// Schema:
//   - chunks_meta : TEXT-keyed metadata table (chunk_id PK, episode/podcast/
//                   speaker FKs, start/end ms, text). Sourced for FTS
//                   bodies and `Chunk` reconstruction.
//   - chunks_vec  : vec0 virtual table over a 1024-d cosine embedding,
//                   keyed by `chunk_id` TEXT. The cosine metric matches
//                   `text-embedding-3-large` at 1024 dimensions.
//   - chunks_fts  : fts5 virtual table over `text`, keyed externally by
//                   `chunk_id` UNINDEXED. Drives BM25 lexical search and
//                   the highlight ranges returned by `hybridTopK`.
//
// All three are kept in lockstep via one transaction per `upsert(...)`.

// MARK: - Public protocol

/// On-device store for embedded chunks of transcripts and wiki pages.
///
/// Implementations must be safe to call from any task. The SQLiteVec impl
/// runs on a serialized actor; the in-memory fallback is also actor-isolated.
/// All methods are async to leave room for future remote stores without
/// changing the call sites in `RAGSearch`, Lane 7 (wiki indexer), and
/// Lane 10 (`query_transcripts` / `query_wiki` agent tools).
///
/// **Embedding production is owned by the store**, not the caller. The
/// `Chunk` struct intentionally has no embedding field — callers (Lane 7
/// wiki indexer, Lane 10 agent tools) only ever read text + metadata.
/// At upsert time the store embeds chunk text via an injected
/// `EmbeddingsClient`. At query time callers pre-embed the *query* string
/// (typically via `RAGSearch`, which lives one layer above).
protocol VectorStore: Sendable {
    /// Insert or replace chunks. The store itself embeds each `chunk.text`
    /// before writing into the vector index.
    func upsert(chunks: [Chunk]) async throws

    /// Drop every chunk for the given episode. Idempotent.
    func deleteAll(forEpisodeID: UUID) async throws

    /// Pure cosine top-K over the vector index.
    /// Score is `1 - distance`, so higher is better.
    func topK(
        _ k: Int,
        for queryVector: [Float],
        scope: ChunkScope?
    ) async throws -> [ChunkMatch]

    /// Hybrid: cosine top-K from `vec0` MERGED with BM25 top-K from `fts5`
    /// via Reciprocal Rank Fusion. Returns up to `k` results with FTS-derived
    /// highlight ranges populated.
    func hybridTopK(
        _ k: Int,
        query: String,
        queryVector: [Float],
        scope: ChunkScope?
    ) async throws -> [ChunkMatch]
}

// MARK: - Errors

enum VectorStoreError: LocalizedError {
    case dimensionMismatch(expected: Int, got: Int)
    case storeNotInitialized
    case backingStorageFailure(String)

    var errorDescription: String? {
        switch self {
        case let .dimensionMismatch(expected, got):
            return "Embedding dimension mismatch: expected \(expected), got \(got)."
        case .storeNotInitialized:
            return "Vector store has not been initialized."
        case let .backingStorageFailure(detail):
            return "Vector store storage failure: \(detail)"
        }
    }
}

// MARK: - SQLiteVec implementation

/// Production `VectorStore` backed by `sqlite-vec` + `fts5`.
///
/// `actor` isolation: SQLiteVec's `Database` is itself an actor, but we wrap
/// it so we can serialize multi-statement transactions (upsert touches three
/// tables per chunk) and so the public surface composes cleanly with Swift 6
/// strict concurrency.
actor VectorIndex: VectorStore {
    static let embeddingDimensions = 1024

    private static let logger = Logger.app("VectorIndex")
    private let db: Database
    private let dimensions: Int
    private let embedder: EmbeddingsClient
    private var schemaReady: Bool = false

    /// Open (or create) the on-disk store. Defaults to
    /// `$applicationSupport/podcastr/vectors.sqlite`. The `embedder` is
    /// invoked on every `upsert(chunks:)` to produce vectors from text.
    ///
    /// Pass `fileURL = nil` for the default on-disk path. Pass
    /// `inMemory: true` for an ephemeral DB. When `inMemory` is true,
    /// `fileURL` is ignored. We don't accept `Database.Location` directly
    /// because it isn't `Sendable` in SQLiteVec 0.0.14, which trips strict
    /// concurrency at the actor boundary.
    init(
        embedder: EmbeddingsClient,
        fileURL: URL? = nil,
        inMemory: Bool = false,
        dimensions: Int = VectorIndex.embeddingDimensions
    ) throws {
        try SQLiteVec.initialize()
        self.dimensions = dimensions
        self.embedder = embedder

        if inMemory {
            self.db = try Database(.inMemory)
        } else {
            let url = try fileURL ?? Self.defaultStoreURL()
            self.db = try Database(.uri(url.path))
        }
    }

    /// Resolve (and create) the persistent on-disk path for the store.
    /// `applicationSupportDirectory` is **not** auto-created on iOS, so we
    /// have to ensure the `podcastr/` subdirectory exists before SQLite tries
    /// to open the file.
    static func defaultStoreURL() throws -> URL {
        let fm = FileManager.default
        let support = try fm.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let dir = support.appendingPathComponent("podcastr", isDirectory: true)
        if !fm.fileExists(atPath: dir.path) {
            try fm.createDirectory(at: dir, withIntermediateDirectories: true)
        }
        return dir.appendingPathComponent("vectors.sqlite")
    }

    // MARK: VectorStore

    func upsert(chunks: [Chunk]) async throws {
        try await ensureSchema()
        guard !chunks.isEmpty else { return }

        // Embed all chunk texts up front (the embedder batches internally).
        let texts = chunks.map(\.text)
        let vectors = try await embedder.embed(texts)
        guard vectors.count == chunks.count else {
            throw VectorStoreError.backingStorageFailure(
                "Embedder returned \(vectors.count) vectors for \(chunks.count) chunks")
        }
        for vec in vectors where vec.count != dimensions {
            throw VectorStoreError.dimensionMismatch(expected: dimensions, got: vec.count)
        }

        // We can't use `db.transaction { ... }` here: its closure parameter
        // isn't `@Sendable`, so under Swift 6 strict concurrency the actor
        // boundary on `Database` rejects our captures. Manual BEGIN/COMMIT
        // gives the same atomicity without crossing a non-Sendable closure.
        _ = try await db.execute("BEGIN TRANSACTION")
        do {
            for (chunk, vec) in zip(chunks, vectors) {
                try await upsertOne(chunk: chunk, vector: vec)
            }
            _ = try await db.execute("COMMIT TRANSACTION")
        } catch {
            _ = try? await db.execute("ROLLBACK TRANSACTION")
            throw error
        }
    }

    private func upsertOne(chunk: Chunk, vector: [Float]) async throws {
        let cid = chunk.id.uuidString
        _ = try await db.execute(
            "DELETE FROM chunks_meta WHERE chunk_id = ?", params: [cid])
        _ = try await db.execute(
            "DELETE FROM chunks_vec WHERE chunk_id = ?", params: [cid])
        _ = try await db.execute(
            "DELETE FROM chunks_fts WHERE chunk_id = ?", params: [cid])
        _ = try await db.execute(
            """
            INSERT INTO chunks_meta(
                chunk_id, episode_id, podcast_id, speaker_id,
                start_ms, end_ms, text
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            params: [
                cid,
                chunk.episodeID.uuidString,
                chunk.podcastID.uuidString,
                chunk.speakerID?.uuidString ?? "",
                chunk.startMS,
                chunk.endMS,
                chunk.text,
            ]
        )
        _ = try await db.execute(
            "INSERT INTO chunks_vec(chunk_id, embedding) VALUES (?, ?)",
            params: [cid, vector]
        )
        _ = try await db.execute(
            "INSERT INTO chunks_fts(chunk_id, text) VALUES (?, ?)",
            params: [cid, chunk.text]
        )
    }

    func deleteAll(forEpisodeID episodeID: UUID) async throws {
        try await ensureSchema()
        let eid = episodeID.uuidString
        _ = try await db.execute("BEGIN TRANSACTION")
        do {
            let rows = try await db.query(
                "SELECT chunk_id FROM chunks_meta WHERE episode_id = ?", params: [eid])
            let cids = rows.compactMap { $0["chunk_id"] as? String }
            for cid in cids {
                _ = try await db.execute(
                    "DELETE FROM chunks_vec WHERE chunk_id = ?", params: [cid])
                _ = try await db.execute(
                    "DELETE FROM chunks_fts WHERE chunk_id = ?", params: [cid])
            }
            _ = try await db.execute(
                "DELETE FROM chunks_meta WHERE episode_id = ?", params: [eid])
            _ = try await db.execute("COMMIT TRANSACTION")
        } catch {
            _ = try? await db.execute("ROLLBACK TRANSACTION")
            throw error
        }
    }

    func chunk(episodeID: UUID, overlappingStartMS startMS: Int, endMS: Int) async throws -> Chunk? {
        try await ensureSchema()
        let rows = try await db.query(
            """
            SELECT chunk_id, episode_id, podcast_id, speaker_id, start_ms, end_ms, text
            FROM chunks_meta
            WHERE episode_id = ? AND start_ms < ? AND end_ms > ?
            ORDER BY ABS(start_ms - ?), start_ms
            LIMIT 1
            """,
            params: [episodeID.uuidString, endMS, startMS, startMS]
        )
        guard let row = rows.first,
              let cid = row["chunk_id"] as? String,
              let eid = (row["episode_id"] as? String).flatMap(UUID.init),
              let pid = (row["podcast_id"] as? String).flatMap(UUID.init),
              let rowStart = row["start_ms"] as? Int,
              let rowEnd = row["end_ms"] as? Int,
              let text = row["text"] as? String,
              let id = UUID(uuidString: cid) else { return nil }
        let speaker = (row["speaker_id"] as? String).flatMap { $0.isEmpty ? nil : UUID(uuidString: $0) }
        return Chunk(id: id, episodeID: eid, podcastID: pid, text: text, startMS: rowStart, endMS: rowEnd, speakerID: speaker)
    }

    func topK(
        _ k: Int,
        for queryVector: [Float],
        scope: ChunkScope?
    ) async throws -> [ChunkMatch] {
        try await ensureSchema()
        guard queryVector.count == dimensions else {
            throw VectorStoreError.dimensionMismatch(expected: dimensions, got: queryVector.count)
        }
        // Ask vec0 for an over-fetch and filter post hoc by scope. vec0
        // doesn't support `WHERE` predicates against unindexed metadata, so
        // we widen the candidate window and intersect with chunks_meta.
        let overfetch = max(k * 4, k + 16)
        let vecRows = try await db.query(
            """
            SELECT chunk_id, distance
            FROM chunks_vec
            WHERE embedding MATCH ?
            ORDER BY distance
            LIMIT ?
            """,
            params: [queryVector, overfetch]
        )
        let candidates: [(cid: String, distance: Double)] = vecRows.compactMap { row in
            guard let cid = row["chunk_id"] as? String else { return nil }
            let distance = (row["distance"] as? Double) ?? Double((row["distance"] as? Float) ?? 0)
            return (cid, distance)
        }
        guard !candidates.isEmpty else { return [] }

        let cids = candidates.map(\.cid)
        let metas = try await fetchMetadata(forChunkIDs: cids, scope: scope)
        var matches: [ChunkMatch] = []
        for (cid, distance) in candidates {
            guard let chunk = metas[cid] else { continue }
            let score = Float(max(0.0, 1.0 - distance))
            matches.append(ChunkMatch(chunk: chunk, score: score))
            if matches.count >= k { break }
        }
        return matches
    }

    func hybridTopK(
        _ k: Int,
        query: String,
        queryVector: [Float],
        scope: ChunkScope?
    ) async throws -> [ChunkMatch] {
        try await ensureSchema()
        guard queryVector.count == dimensions else {
            throw VectorStoreError.dimensionMismatch(expected: dimensions, got: queryVector.count)
        }

        let overfetch = max(k * 4, k + 16)

        // Vector candidates.
        let vecRows = try await db.query(
            """
            SELECT chunk_id, distance
            FROM chunks_vec
            WHERE embedding MATCH ?
            ORDER BY distance
            LIMIT ?
            """,
            params: [queryVector, overfetch]
        )
        let vecOrder: [String] = vecRows.compactMap { $0["chunk_id"] as? String }

        // FTS candidates.
        let ftsQuery = Self.sanitizeFTSQuery(query)
        var ftsOrder: [String] = []
        if !ftsQuery.isEmpty {
            let ftsRows = try await db.query(
                """
                SELECT chunk_id, bm25(chunks_fts) AS score
                FROM chunks_fts
                WHERE chunks_fts MATCH ?
                ORDER BY score
                LIMIT ?
                """,
                params: [ftsQuery, overfetch]
            )
            ftsOrder = ftsRows.compactMap { $0["chunk_id"] as? String }
        }

        // Reciprocal Rank Fusion (k = 60 per Garcia / standard).
        let fused = Self.rrf(vecRanks: vecOrder, ftsRanks: ftsOrder)
        guard !fused.isEmpty else { return [] }

        let cids = fused.map(\.cid)
        let metas = try await fetchMetadata(forChunkIDs: cids, scope: scope)
        var matches: [ChunkMatch] = []
        for (cid, score) in fused {
            guard let chunk = metas[cid] else { continue }
            let highlights = ChunkHighlights.compute(in: chunk.text, query: query)
            matches.append(ChunkMatch(chunk: chunk, score: score, textHighlights: highlights))
            if matches.count >= k { break }
        }
        return matches
    }

    // MARK: Schema

    private func ensureSchema() async throws {
        if schemaReady { return }
        // chunks_meta: ordinary table — needed because vec0 / fts5 are
        // virtual tables and their rowids don't compose with WHERE on
        // arbitrary columns.
        _ = try await db.execute(
            """
            CREATE TABLE IF NOT EXISTS chunks_meta(
                chunk_id   TEXT PRIMARY KEY,
                episode_id TEXT NOT NULL,
                podcast_id TEXT NOT NULL,
                speaker_id TEXT,
                start_ms   INTEGER NOT NULL,
                end_ms     INTEGER NOT NULL,
                text       TEXT NOT NULL
            )
            """
        )
        _ = try await db.execute(
            "CREATE INDEX IF NOT EXISTS idx_chunks_meta_episode ON chunks_meta(episode_id)")
        _ = try await db.execute(
            "CREATE INDEX IF NOT EXISTS idx_chunks_meta_podcast ON chunks_meta(podcast_id)")
        _ = try await db.execute(
            "CREATE INDEX IF NOT EXISTS idx_chunks_meta_speaker ON chunks_meta(speaker_id)")

        _ = try await db.execute(
            """
            CREATE VIRTUAL TABLE IF NOT EXISTS chunks_vec USING vec0(
                chunk_id TEXT PRIMARY KEY,
                embedding FLOAT[\(dimensions)] distance_metric=cosine
            )
            """
        )
        _ = try await db.execute(
            """
            CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
                chunk_id UNINDEXED,
                text,
                tokenize='porter'
            )
            """
        )
        schemaReady = true
    }

    // MARK: Metadata fetch + scope filter

    private func fetchMetadata(
        forChunkIDs cids: [String],
        scope: ChunkScope?
    ) async throws -> [String: Chunk] {
        guard !cids.isEmpty else { return [:] }
        let placeholders = Array(repeating: "?", count: cids.count).joined(separator: ",")
        var params: [any Sendable] = cids
        var sql =
            "SELECT chunk_id, episode_id, podcast_id, speaker_id, start_ms, end_ms, text FROM chunks_meta WHERE chunk_id IN (\(placeholders))"
        if let scope {
            switch scope {
            case .all:
                break
            case let .podcast(pid):
                sql += " AND podcast_id = ?"
                params.append(pid.uuidString)
            case let .episodes(ids):
                guard !ids.isEmpty else { return [:] }
                let episodePlaceholders = Array(repeating: "?", count: ids.count).joined(separator: ",")
                sql += " AND episode_id IN (\(episodePlaceholders))"
                params.append(contentsOf: ids.map(\.uuidString))
            case let .episode(eid):
                sql += " AND episode_id = ?"
                params.append(eid.uuidString)
            case let .speaker(sid):
                sql += " AND speaker_id = ?"
                params.append(sid.uuidString)
            }
        }
        let rows = try await db.query(sql, params: params)
        var out: [String: Chunk] = [:]
        for row in rows {
            guard let cid = row["chunk_id"] as? String,
                  let eid = (row["episode_id"] as? String).flatMap(UUID.init),
                  let pid = (row["podcast_id"] as? String).flatMap(UUID.init),
                  let startMS = row["start_ms"] as? Int,
                  let endMS = row["end_ms"] as? Int,
                  let text = row["text"] as? String,
                  let id = UUID(uuidString: cid) else { continue }
            let speaker = (row["speaker_id"] as? String).flatMap { $0.isEmpty ? nil : UUID(uuidString: $0) }
            out[cid] = Chunk(
                id: id,
                episodeID: eid,
                podcastID: pid,
                text: text,
                startMS: startMS,
                endMS: endMS,
                speakerID: speaker
            )
        }
        return out
    }

    // MARK: RRF + highlight helpers (static, pure)

    static func rrf(vecRanks: [String], ftsRanks: [String], k: Double = 60) -> [(cid: String, score: Float)] {
        var scores: [String: Double] = [:]
        for (i, cid) in vecRanks.enumerated() {
            scores[cid, default: 0] += 1.0 / (k + Double(i + 1))
        }
        for (i, cid) in ftsRanks.enumerated() {
            scores[cid, default: 0] += 1.0 / (k + Double(i + 1))
        }
        return scores
            .sorted { $0.value > $1.value }
            .map { (cid: $0.key, score: Float($0.value)) }
    }

    /// Strip characters FTS5 treats as syntax so user text can be passed
    /// verbatim. Keeps alphanumerics + whitespace; everything else becomes
    /// a space. Empty-after-strip → no FTS query.
    static func sanitizeFTSQuery(_ raw: String) -> String {
        let cleaned = raw.unicodeScalars.map { scalar -> Character in
            if CharacterSet.alphanumerics.contains(scalar) || scalar == " " {
                return Character(scalar)
            }
            return " "
        }
        let s = String(cleaned).trimmingCharacters(in: .whitespacesAndNewlines)
        return s
    }

}
