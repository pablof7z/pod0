import Foundation
import Pod0Core

/// Maps Rust-owned evidence projections into the existing agent episode
/// presentation values. It never chunks, ranks, cites, or selects artifacts.
struct LivePodcastKnowledgeAdapter: PodcastAgentKnowledgeSearchProtocol {
    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    func searchEpisodes(query: String, scope: PodcastID?, limit: Int) async throws -> [EpisodeHit] {
        let coreScope = await episodeSearchScope(scope)
        let projection = await self.query(query, scope: coreScope, limit: limit)
        guard projection.stage == .ready else {
            if projection.stage == .noEvidence { return [] }
            throw SharedKnowledgeSearchError(stage: projection.stage)
        }
        return await episodeHits(from: projection.evidence, excluding: nil, limit: limit)
    }

    func queryTranscriptEvidence(
        query: String,
        scope: String?,
        limit: Int
    ) async -> RecallResultProjection {
        let coreScope = await transcriptScope(scope)
        return await self.query(query, scope: coreScope, limit: limit)
    }

    func findSimilarEpisodes(seedEpisodeID: EpisodeID, k: Int) async throws -> [EpisodeHit] {
        guard let seedUUID = UUID(uuidString: seedEpisodeID),
              let seed = await store?.episode(id: seedUUID) else { return [] }
        let query = [seed.title, String(seed.description.prefix(400))]
            .filter { !$0.isEmpty }
            .joined(separator: " ")
        let projection = await self.query(query, scope: .library, limit: max(1, k + 1))
        guard projection.stage == .ready else {
            if projection.stage == .noEvidence { return [] }
            throw SharedKnowledgeSearchError(stage: projection.stage)
        }
        return await episodeHits(from: projection.evidence, excluding: seedUUID, limit: k)
    }

    private func query(
        _ text: String,
        scope: RecallScope,
        limit: Int
    ) async -> RecallResultProjection {
        guard let client = await store?.sharedLibrary else {
            return RecallResultProjection.interrupted()
        }
        return await client.recall(
            query: text,
            scope: scope,
            limit: UInt16(clamping: max(1, min(limit, 20)))
        )
    }

    @MainActor
    private func episodeSearchScope(_ scope: PodcastID?) -> RecallScope {
        guard let scope else { return .library }
        guard let id = UUID(uuidString: scope) else { return .unsupported(wireCode: 1) }
        return .podcast(podcastId: PodcastId(uuid: id))
    }

    @MainActor
    private func transcriptScope(_ raw: String?) -> RecallScope {
        guard let raw else { return .library }
        guard let id = UUID(uuidString: raw) else { return .unsupported(wireCode: 1) }
        if store?.episode(id: id) != nil {
            return .episode(episodeId: EpisodeId(uuid: id))
        }
        if store?.podcast(id: id) != nil {
            return .podcast(podcastId: PodcastId(uuid: id))
        }
        return .episode(episodeId: EpisodeId(uuid: id))
    }

    @MainActor
    private func episodeHits(
        from evidence: [RecallEvidenceProjection],
        excluding excludedID: UUID?,
        limit: Int
    ) -> [EpisodeHit] {
        guard let store else { return [] }
        var seen: Set<UUID> = []
        var hits: [EpisodeHit] = []
        for item in evidence {
            guard let episodeID = item.episodeId.uuid,
                  episodeID != excludedID,
                  seen.insert(episodeID).inserted,
                  let episode = store.episode(id: episodeID) else { continue }
            let podcast = store.podcast(id: episode.podcastID)
            hits.append(EpisodeHit(
                episodeID: episodeID.uuidString,
                podcastID: episode.podcastID.uuidString,
                title: episode.title,
                podcastTitle: podcast?.title ?? "",
                publishedAt: episode.pubDate,
                durationSeconds: episode.duration.map(Int.init),
                snippet: String(item.excerpt.prefix(280)),
                score: Double(item.score.totalRrfUnits)
            ))
            if hits.count >= max(1, limit) { break }
        }
        return hits
    }
}

private struct SharedKnowledgeSearchError: LocalizedError {
    let stage: RecallStage

    var errorDescription: String? {
        "Shared knowledge search ended in \(stage.stableName)."
    }
}
