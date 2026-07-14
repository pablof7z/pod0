import Foundation

// MARK: - RAG search protocol

/// The contract Lane 6's vector index satisfies for the wiki generator.
///
/// Defined here (Lane 7) so the generator pipeline compiles and tests
/// against an in-memory mock without depending on Lane 6's concrete
/// `RAGSearch`. Lane 6 is expected to ship a type that conforms to this
/// protocol; the dependency direction is *inverted* through the protocol
/// so neither lane blocks the other.
///
/// The query model is the minimum surface area the generator needs:
///   1. Find candidate transcript chunks for a topic.
///   2. Verify a synthesized claim against the spans the generator
///      told us it relied on (exact-span lookup).
protocol WikiRAGSearchProtocol: Sendable {

    /// Returns the top-`k` transcript chunks relevant to `query`, scoped
    /// to `scope` (or unscoped when `nil`). Implementations should run
    /// hybrid lexical+vector search and return chunks ordered by score
    /// descending.
    func search(
        query: String,
        scope: WikiScope?,
        limit: Int
    ) async throws -> [RAGChunk]

    /// Looks up the chunk that contains the supplied millisecond span
    /// inside `episodeID`, if any. Used by the verification pass: a
    /// citation that resolves to no chunk is treated as fabricated and
    /// the surrounding claim is dropped.
    func chunk(
        episodeID: UUID,
        startMS: Int,
        endMS: Int
    ) async throws -> RAGChunk?
}

// MARK: - RAG chunk

/// A single retrieval result. Matches the shape emitted by Lane 6's RAG
/// pipeline (sliding-window transcript chunks, ~30–45 seconds of speech
/// per the embeddings-rag-stack research note).
struct RAGChunk: Codable, Hashable, Identifiable, Sendable {

    var id: UUID
    var episodeID: UUID
    var podcastID: UUID
    var startMS: Int
    var endMS: Int
    var text: String
    var speaker: String?
    /// Cosine similarity (or RRF score) — 0…1 normalised by Lane 6.
    var score: Double

    init(
        id: UUID = UUID(),
        episodeID: UUID,
        podcastID: UUID,
        startMS: Int,
        endMS: Int,
        text: String,
        speaker: String? = nil,
        score: Double = 0
    ) {
        self.id = id
        self.episodeID = episodeID
        self.podcastID = podcastID
        self.startMS = startMS
        self.endMS = endMS
        self.text = text
        self.speaker = speaker
        self.score = score
    }

    /// `true` when `[startMS, endMS)` overlaps `[other.startMS, other.endMS)`.
    /// Used by the verifier to match a citation to a real chunk even when
    /// the LLM picked a slightly off-by-a-second span.
    func overlaps(startMS: Int, endMS: Int) -> Bool {
        startMS < self.endMS && endMS > self.startMS
    }
}

// MARK: - In-memory RAG search

/// Test/preview implementation backed by a fixed set of `RAGChunk`s.
/// Useful for SwiftUI previews and the lane-7 generator unit tests.
struct InMemoryRAGSearch: WikiRAGSearchProtocol {

    var chunks: [RAGChunk]

    init(chunks: [RAGChunk] = []) {
        self.chunks = chunks
    }

    func search(query: String, scope: WikiScope?, limit: Int) async throws -> [RAGChunk] {
        let lowercaseQuery = query.lowercased()
        let scoped = chunks.filter { chunk in
            switch scope {
            case .none, .global?:
                return true
            case .podcast(let id)?:
                return chunk.podcastID == id
            }
        }
        let scored = scoped
            .map { chunk -> (chunk: RAGChunk, hits: Int) in
                let hits = lowercaseQuery
                    .split(whereSeparator: { !$0.isLetter && !$0.isNumber })
                    .filter { chunk.text.lowercased().contains($0) }
                    .count
                return (chunk, hits)
            }
            .filter { $0.hits > 0 }
            .sorted { $0.hits > $1.hits }
            .prefix(limit)
        return scored.map(\.chunk)
    }

    func chunk(episodeID: UUID, startMS: Int, endMS: Int) async throws -> RAGChunk? {
        chunks.first { chunk in
            chunk.episodeID == episodeID && chunk.overlaps(startMS: startMS, endMS: endMS)
        }
    }
}
