import Foundation

// Lane 6 — RAG: hand-rolled in-memory `VectorStore` fallback.
//
// Used when:
//   1. `SQLiteVec` SPM resolution fails (offline build, registry issue), or
//   2. tests/previews need a lightweight store that doesn't touch SQLite.
//
// Storage is a flat array of (Chunk, [Float]) tuples kept on the actor.
// Cosine similarity is computed by hand; for the ~50K-chunk power-user
// target this is a few megs of RAM and a brute-force scan that completes
// in well under 100ms on M-class chips per the research note's
// quantization-free baseline. NOT suitable as a long-term replacement
// — this exists purely to keep the build green and the agent path
// runnable when the SQLite path is unavailable.

actor InMemoryVectorStore: VectorStore {
    private struct Entry {
        let chunk: Chunk
        let embedding: [Float]
        // Cached norm so cosine similarity is one dot-product per query
        // candidate plus one division.
        let norm: Float
    }

    private let embedder: EmbeddingsClient
    private var entries: [Entry] = []

    init(embedder: EmbeddingsClient) {
        self.embedder = embedder
    }

    func upsert(chunks: [Chunk]) async throws {
        guard !chunks.isEmpty else { return }
        let vectors = try await embedder.embed(chunks.map(\.text))
        guard vectors.count == chunks.count else {
            throw VectorStoreError.backingStorageFailure(
                "Embedder returned \(vectors.count) for \(chunks.count) chunks")
        }
        // Drop existing entries for these chunk IDs (idempotent upsert).
        let incomingIDs = Set(chunks.map(\.id))
        entries.removeAll { incomingIDs.contains($0.chunk.id) }
        for (chunk, vec) in zip(chunks, vectors) {
            let norm = sqrt(vec.reduce(0) { $0 + $1 * $1 })
            entries.append(Entry(chunk: chunk, embedding: vec, norm: max(norm, .leastNormalMagnitude)))
        }
    }

    func deleteAll(forEpisodeID episodeID: UUID) async throws {
        entries.removeAll { $0.chunk.episodeID == episodeID }
    }

    func topK(
        _ k: Int,
        for queryVector: [Float],
        scope: ChunkScope?
    ) async throws -> [ChunkMatch] {
        guard !entries.isEmpty else { return [] }
        let qNorm = sqrt(queryVector.reduce(0) { $0 + $1 * $1 })
        let denom = max(qNorm, .leastNormalMagnitude)

        var scored: [(ChunkMatch, Float)] = []
        scored.reserveCapacity(entries.count)
        for entry in entries where Self.matches(scope: scope, chunk: entry.chunk) {
            guard entry.embedding.count == queryVector.count else { continue }
            var dot: Float = 0
            for i in 0..<queryVector.count {
                dot += entry.embedding[i] * queryVector[i]
            }
            let cosine = dot / (entry.norm * denom)
            scored.append((ChunkMatch(chunk: entry.chunk, score: cosine), cosine))
        }
        return scored
            .sorted { $0.1 > $1.1 }
            .prefix(k)
            .map(\.0)
    }

    func hybridTopK(
        _ k: Int,
        query: String,
        queryVector: [Float],
        scope: ChunkScope?
    ) async throws -> [ChunkMatch] {
        // Vector candidates.
        let vecMatches = try await topK(
            max(k * 4, k + 16), for: queryVector, scope: scope)
        let vecOrder: [String] = vecMatches.map { $0.chunk.id.uuidString }

        // Lexical candidates: cheap token-overlap score (Jaccard-ish).
        let qTokens = Set(Self.tokenize(query))
        var lex: [(String, Double)] = []
        if !qTokens.isEmpty {
            for entry in entries where Self.matches(scope: scope, chunk: entry.chunk) {
                let docTokens = Set(Self.tokenize(entry.chunk.text))
                let overlap = qTokens.intersection(docTokens).count
                guard overlap > 0 else { continue }
                let score = Double(overlap) / Double(qTokens.union(docTokens).count)
                lex.append((entry.chunk.id.uuidString, score))
            }
        }
        let lexOrder: [String] = lex
            .sorted { $0.1 > $1.1 }
            .prefix(max(k * 4, k + 16))
            .map(\.0)

        // RRF.
        var rrfScores: [String: Double] = [:]
        for (i, cid) in vecOrder.enumerated() {
            rrfScores[cid, default: 0] += 1.0 / (60.0 + Double(i + 1))
        }
        for (i, cid) in lexOrder.enumerated() {
            rrfScores[cid, default: 0] += 1.0 / (60.0 + Double(i + 1))
        }

        let byID: [String: Chunk] = Dictionary(
            uniqueKeysWithValues: entries.map { ($0.chunk.id.uuidString, $0.chunk) })

        let merged = rrfScores
            .sorted { $0.value > $1.value }
            .prefix(k)
            .compactMap { (cid, score) -> ChunkMatch? in
                guard let chunk = byID[cid] else { return nil }
                let highlights = ChunkHighlights.compute(in: chunk.text, query: query)
                return ChunkMatch(chunk: chunk, score: Float(score), textHighlights: highlights)
            }
        return Array(merged)
    }

    // MARK: - Helpers

    /// True when `chunk` passes `scope`. nil scope = match everything.
    static func matches(scope: ChunkScope?, chunk: Chunk) -> Bool {
        guard let scope else { return true }
        switch scope {
        case .all:                  return true
        case let .podcast(id):      return chunk.podcastID == id
        case let .episodes(ids):    return ids.contains(chunk.episodeID)
        case let .episode(id):      return chunk.episodeID == id
        case let .speaker(id):      return chunk.speakerID == id
        }
    }

    static func tokenize(_ text: String) -> [String] {
        text
            .lowercased()
            .split { !$0.isLetter && !$0.isNumber }
            .map(String.init)
            .filter { $0.count >= 3 }
    }
}
