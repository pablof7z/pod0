import Foundation

// MARK: - LivePodcastRAGAdapter
//
// Bridges `KernelKnowledgeClient` (which calls `nmp_app_podcast_knowledge_query`)
// to the agent-tool's `[EpisodeHit]` / `[TranscriptHit]` value types.
//
// Slice 5d: all three adapter methods now call the kernel knowledge-query FFI
// (slice 5b) instead of `RAGService.shared.search`. The Swift RAGSearch stack
// is kept dormant (deleted in slice 5f).
//
// `findSimilarEpisodes` is implemented as a semantic query using the seed
// episode's own title + description excerpt with no scope, then filtering the
// seed from results. There is no dedicated "similar" FFI in slice 5b; this
// approximates similarity via the seed episode's own text.
//
// `queryTranscripts` passes speaker=nil — the kernel chunk row carries no
// diarisation field; speaker attribution is a future extension.

struct LivePodcastRAGAdapter: PodcastAgentRAGSearchProtocol {

    /// Weak handle on the live store — used to hydrate optional per-episode
    /// metadata (publishedAt, durationSeconds) and to disambiguate scope UUIDs
    /// in `queryTranscripts`. Does not gate the core search path; if nil the
    /// optional fields are omitted from results.
    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    // MARK: - PodcastAgentRAGSearchProtocol

    func searchEpisodes(query: String, scope: PodcastID?, limit: Int) async throws -> [EpisodeHit] {
        // Over-fetch so the per-episode rollup still returns `limit` distinct
        // episodes when several chunks come from the same show.
        let rows = try await KernelKnowledgeClient.query(
            query: query,
            podcastId: scope,
            episodeId: nil,
            limit: max(1, limit) * 4
        )
        return await rollUpToEpisodes(rows: rows, limit: limit)
    }

    func queryTranscripts(query: String, scope: String?, limit: Int) async throws -> [TranscriptHit] {
        // Resolve scope UUID → podcast or episode on the main actor (store
        // access), then call the FFI off-main via KernelKnowledgeClient.
        let (podcastId, episodeId) = await MainActor.run { [store] in
            Self.resolveTranscriptScope(scope: scope, store: store)
        }
        let rows = try await KernelKnowledgeClient.query(
            query: query,
            podcastId: podcastId,
            episodeId: episodeId,
            limit: max(1, limit)
        )
        return rows.map(Self.makeTranscriptHit)
    }

    func findSimilarEpisodes(seedEpisodeID: EpisodeID, k: Int) async throws -> [EpisodeHit] {
        // Build the retrieval query from the seed episode's metadata on the
        // main actor, then issue a library-wide semantic search (no scope).
        // Limitation: no dedicated "similar" FFI exists in slice 5b; this
        // approximates similarity using the seed's own title + description text.
        let seedQuery = await MainActor.run { [store] in
            guard let uuid = UUID(uuidString: seedEpisodeID),
                  let ep = store?.episode(id: uuid) else { return "" }
            return [ep.title, String(ep.description.prefix(400))]
                .filter { !$0.isEmpty }
                .joined(separator: " ")
        }
        guard !seedQuery.isEmpty else { return [] }

        let rows = try await KernelKnowledgeClient.query(
            query: seedQuery,
            podcastId: nil,
            episodeId: nil,
            limit: max(1, k) * 4
        )
        let hits = await rollUpToEpisodes(rows: rows, limit: k + 1)
        return Array(hits.filter { $0.episodeID != seedEpisodeID }.prefix(k))
    }

    // MARK: - Private rollup

    /// Collapse chunk rows to episode-level hits, keeping the best-scoring
    /// chunk snippet per episode. Hydrates publishedAt/durationSeconds from the
    /// store (best-effort — both are nil when the store doesn't hold the episode).
    @MainActor
    private func rollUpToEpisodes(rows: [KnowledgeQueryRow], limit: Int) -> [EpisodeHit] {
        var bestPerEpisode: [String: KnowledgeQueryRow] = [:]
        var orderedIDs: [String] = []

        for row in rows {
            if let prior = bestPerEpisode[row.episodeId] {
                if row.relevanceScore > prior.relevanceScore {
                    bestPerEpisode[row.episodeId] = row
                }
            } else {
                orderedIDs.append(row.episodeId)
                bestPerEpisode[row.episodeId] = row
            }
            if orderedIDs.count >= limit { break }
        }

        return orderedIDs.prefix(limit).compactMap { id -> EpisodeHit? in
            guard let row = bestPerEpisode[id] else { return nil }
            var pubDate: Date?
            var duration: Int?
            if let uuid = UUID(uuidString: id), let ep = store?.episode(id: uuid) {
                pubDate = ep.pubDate
                duration = ep.duration.map { Int($0) }
            }
            return EpisodeHit(
                episodeID: row.episodeId,
                podcastID: row.podcastId,
                title: row.episodeTitle,
                podcastTitle: row.podcastTitle,
                publishedAt: pubDate,
                durationSeconds: duration,
                snippet: String(row.text.prefix(280)),
                score: row.relevanceScore
            )
        }
    }

    // MARK: - Helpers

    /// Map a `KnowledgeQueryRow` chunk to a `TranscriptHit`.
    /// `speaker` is `nil` — kernel chunk rows carry no diarisation field.
    private static func makeTranscriptHit(_ row: KnowledgeQueryRow) -> TranscriptHit {
        TranscriptHit(
            episodeID: row.episodeId,
            startSeconds: row.startSecs,
            endSeconds: row.endSecs,
            speaker: nil,
            text: row.text,
            score: row.relevanceScore
        )
    }

    /// Disambiguate a scope UUID string as either a podcast_id or episode_id.
    ///
    /// Resolution order (mirrors the legacy `ChunkScope` logic):
    /// 1. Episode-first: a UUID that resolves via `store.episode(id:)` → episode scope.
    /// 2. Subscription: a UUID present in `store.state.subscriptions` → podcast scope.
    /// 3. Default fallback: treat as episode scope (defensive — narrows rather than widens).
    @MainActor
    private static func resolveTranscriptScope(
        scope: String?,
        store: AppStateStore?
    ) -> (podcastId: String?, episodeId: String?) {
        guard let raw = scope, UUID(uuidString: raw) != nil else {
            return (nil, nil)
        }
        guard let uuid = UUID(uuidString: raw) else { return (nil, nil) }
        if store?.episode(id: uuid) != nil { return (nil, raw) }
        if store?.state.subscriptions.contains(where: { $0.id == uuid }) == true {
            return (raw, nil)
        }
        return (nil, raw)
    }
}
