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

// MARK: - BriefingRAGSearchAdapter
//
// Bridges `RAGSearch` to `BriefingRAGSearchProtocol`. Unlike the wiki adapter,
// this one needs episode + subscription metadata (`enclosureURL`, show name)
// to populate `RAGCandidate.sourceLabel` and `RAGCandidate.enclosureURL`.
// The lookup is satisfied lazily through `RAGService.appStore`.

struct BriefingRAGSearchAdapter: BriefingRAGSearchProtocol {

    let service: RAGService

    func search(
        query: String,
        scope: BriefingScope,
        limit: Int
    ) async throws -> [RAGCandidate] {
        let chunkScope = await Self.chunkScope(for: scope, service: service)
        if case .episodes(let ids) = chunkScope, ids.isEmpty {
            return []
        }
        let options = RAGSearch.Options(k: max(1, limit))
        let matches = try await service.search.search(
            query: query,
            scope: chunkScope,
            options: options
        )
        return await Self.shape(matches: matches, service: service)
    }

    // MARK: - Helpers

    /// Translate the briefing-domain scope to the vector-store-domain scope.
    /// Scopes that cannot be represented by the current request shape resolve
    /// to an empty episode set instead of widening to the whole corpus.
    @MainActor
    static func chunkScope(for briefing: BriefingScope, service: RAGService) -> ChunkScope {
        switch briefing {
        case .mySubscriptions, .thisTopic:
            return .all
        case .thisWeek:
            guard let store = service.appStore else { return .episodes([]) }
            let cutoff = Calendar.current.date(byAdding: .day, value: -7, to: Date()) ?? Date()
            let ids = Set(store.state.episodes.filter { $0.pubDate >= cutoff }.map(\.id))
            return .episodes(ids)
        case .thisShow:
            return .episodes([])
        }
    }

    /// Map vector-store matches into briefing candidates, hydrating
    /// metadata from the `AppStateStore` when available.
    @MainActor
    static func shape(matches: [ChunkMatch], service: RAGService) -> [RAGCandidate] {
        let store = service.appStore
        return matches.map { m -> RAGCandidate in
            let episode = store?.episode(id: m.chunk.episodeID)
            let podcast = episode.flatMap { ep in
                store?.state.podcasts.first { $0.id == ep.podcastID }
            }
            let label: String = {
                let timeMS = m.chunk.startMS
                let formatted = formatTime(seconds: Double(timeMS) / 1000.0)
                if let title = podcast?.title, !title.isEmpty {
                    return "\(title) · \(formatted)"
                }
                if let title = episode?.title, !title.isEmpty {
                    return "\(title) · \(formatted)"
                }
                return "Podcast · \(formatted)"
            }()
            return RAGCandidate(
                id: m.chunk.id,
                sourceKind: .episode,
                episodeID: m.chunk.episodeID,
                enclosureURL: episode?.enclosureURL,
                wikiPageID: nil,
                sourceLabel: label,
                text: m.chunk.text,
                startSeconds: TimeInterval(m.chunk.startMS) / 1000.0,
                endSeconds: TimeInterval(m.chunk.endMS) / 1000.0,
                score: Double(m.score)
            )
        }
    }

    private static func formatTime(seconds: TimeInterval) -> String {
        let total = Int(seconds.rounded())
        let mm = total / 60
        let ss = total % 60
        return String(format: "%d:%02d", mm, ss)
    }
}
