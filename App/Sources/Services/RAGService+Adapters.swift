import Foundation
import os.log

// MARK: - WikiRAGSearchAdapter
//
// Bridges `RAGSearch` (which returns `[ChunkMatch]`) to `WikiRAGSearchProtocol`
// (which the wiki generator + verifier consume as `[RAGChunk]`).
//
// Two responsibilities:
//   1. Score-and-shape conversion (`ChunkMatch.chunk + score → RAGChunk`).
//   2. `WikiScope` ↔ `ChunkScope` translation. The wiki layer knows about
//      `global` and `podcast(UUID)`; the vector store also has `episode` and
//      `speaker`, but the wiki side never asks for those.

struct WikiRAGSearchAdapter: WikiRAGSearchProtocol {

    let search: RAGSearch
    let index: VectorIndex

    func search(
        query: String,
        scope: WikiScope?,
        limit: Int
    ) async throws -> [RAGChunk] {
        let chunkScope = Self.chunkScope(for: scope)
        let options = RAGSearch.Options(k: max(1, limit))
        let matches = try await search.search(
            query: query,
            scope: chunkScope,
            options: options
        )
        return matches.map(Self.makeRAGChunk)
    }

    func chunk(
        episodeID: UUID,
        startMS: Int,
        endMS: Int
    ) async throws -> RAGChunk? {
        if let chunk = try await index.chunk(
            episodeID: episodeID,
            overlappingStartMS: startMS,
            endMS: endMS
        ) {
            return Self.makeRAGChunk(from: ChunkMatch(chunk: chunk, score: 1))
        }
        return nil
    }

    // MARK: - Helpers

    static func chunkScope(for wiki: WikiScope?) -> ChunkScope? {
        guard let wiki else { return nil }
        switch wiki {
        case .global:               return .all
        case .podcast(let id):      return .podcast(id)
        }
    }

    static func makeRAGChunk(from match: ChunkMatch) -> RAGChunk {
        RAGChunk(
            id: match.chunk.id,
            episodeID: match.chunk.episodeID,
            podcastID: match.chunk.podcastID,
            startMS: match.chunk.startMS,
            endMS: match.chunk.endMS,
            text: match.chunk.text,
            speaker: nil,
            score: Double(match.score)
        )
    }
}
