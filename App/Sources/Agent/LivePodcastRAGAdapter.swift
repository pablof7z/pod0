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
// `findSimilarEpisodes` calls the kernel's similar-episode FFI. Rust resolves
// the seed episode, derives the search text, runs retrieval, and filters the
// seed from results.
//
// `queryTranscripts` passes speaker=nil — the kernel chunk row carries no
// diarisation field; speaker attribution is a future extension.

struct LivePodcastRAGAdapter: PodcastAgentRAGSearchProtocol {
    private struct EpisodeRollupEnvelope: Decodable {
        let result: [EpisodeRollupHit]?
        let error: String?
    }

    private struct EpisodeRollupHit: Decodable {
        let episodeID: String
        let podcastID: String
        let title: String
        let podcastTitle: String
        let publishedAt: Int?
        let durationSeconds: Int?
        let snippet: String?
        let score: Double?

        enum CodingKeys: String, CodingKey {
            case title, snippet, score
            case episodeID = "episode_id"
            case podcastID = "podcast_id"
            case podcastTitle = "podcast_title"
            case publishedAt = "published_at"
            case durationSeconds = "duration_seconds"
        }
    }

    private struct TranscriptHitsEnvelope: Decodable {
        let result: [TranscriptHitDTO]?
        let error: String?
    }

    private struct TranscriptHitDTO: Decodable {
        let episodeID: String
        let startSeconds: Double
        let endSeconds: Double
        let speaker: String?
        let text: String
        let score: Double?

        enum CodingKeys: String, CodingKey {
            case episodeID = "episode_id"
            case startSeconds = "start_seconds"
            case endSeconds = "end_seconds"
            case speaker
            case text
            case score
        }

        var hit: TranscriptHit {
            TranscriptHit(
                episodeID: episodeID,
                startSeconds: startSeconds,
                endSeconds: endSeconds,
                speaker: speaker,
                text: text,
                score: score
            )
        }
    }

    /// Weak handle on the live store — used only to hydrate optional
    /// per-episode metadata (publishedAt, durationSeconds). Scope
    /// disambiguation is Rust-owned via `KernelKnowledgeClient.resolveScope`.
    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    // MARK: - PodcastAgentRAGSearchProtocol

    func searchEpisodes(
        query: String,
        scope: PodcastID?,
        limit: Int,
        retrievalLimit: Int
    ) async throws -> [EpisodeHit] {
        let rows = try await KernelKnowledgeClient.query(
            query: query,
            podcastId: scope,
            episodeId: nil,
            limit: retrievalLimit
        )
        return try await rollUpToEpisodes(rows: rows, limit: limit)
    }

    func queryTranscripts(query: String, scope: String?, limit: Int) async throws -> [TranscriptHit] {
        let (podcastId, episodeId) = try await KernelKnowledgeClient.resolveScope(scope)
        let rows = try await KernelKnowledgeClient.query(
            query: query,
            podcastId: podcastId,
            episodeId: episodeId,
            limit: max(1, limit)
        )
        return try await transcriptHits(rows: rows)
    }

    func findSimilarEpisodes(seedEpisodeID: EpisodeID, k: Int) async throws -> [EpisodeHit] {
        let rows = try await KernelKnowledgeClient.similarEpisodes(
            episodeId: seedEpisodeID,
            limit: max(1, k)
        )
        return try await rollUpToEpisodes(rows: rows, limit: k)
    }

    // MARK: - Private rollup

    /// Collapse chunk rows to episode-level hits, keeping the best-scoring
    /// chunk snippet per episode. Hydrates publishedAt/durationSeconds from the
    /// store (best-effort — both are nil when the store doesn't hold the episode).
    private func rollUpToEpisodes(rows: [KnowledgeQueryRow], limit: Int) async throws -> [EpisodeHit] {
        let metadata = await episodeMetadataRows(for: rows)
        let payload: [String: Any] = [
            "op": "episode_rollup",
            "limit": limit,
            "rows": rows.map(Self.rawKnowledgeRow),
            "metadata": metadata,
        ]
        let envelope = try await searchTool(payload: payload)
        guard let result = envelope.result else {
            throw NSError(
                domain: "LivePodcastRAGAdapter",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: envelope.error ?? "Rust episode rollup failed"]
            )
        }
        return result.map { hit in
            EpisodeHit(
                episodeID: hit.episodeID,
                podcastID: hit.podcastID,
                title: hit.title,
                podcastTitle: hit.podcastTitle,
                publishedAt: hit.publishedAt.map { Date(timeIntervalSince1970: TimeInterval($0)) },
                durationSeconds: hit.durationSeconds,
                snippet: hit.snippet,
                score: hit.score
            )
        }
    }

    // MARK: - Helpers

    private func episodeMetadataRows(for rows: [KnowledgeQueryRow]) async -> [[String: Any]] {
        await MainActor.run {
            var seen = Set<String>()
            return rows.compactMap { row -> [String: Any]? in
                guard seen.insert(row.episodeId).inserted,
                      let uuid = UUID(uuidString: row.episodeId),
                      let episode = store?.episode(id: uuid)
                else { return nil }
                var metadata: [String: Any] = ["episode_id": row.episodeId]
                if let pubDate = episode.pubDate {
                    metadata["published_at"] = Int(pubDate.timeIntervalSince1970)
                }
                if let duration = episode.duration {
                    metadata["duration_seconds"] = Int(duration)
                }
                return metadata
            }
        }
    }

    private func transcriptHits(rows: [KnowledgeQueryRow]) async throws -> [TranscriptHit] {
        let payload: [String: Any] = [
            "op": "transcript_hits",
            "rows": rows.map(Self.rawKnowledgeRow),
        ]
        let envelope = try await transcriptHitTool(payload: payload)
        guard let result = envelope.result else {
            throw NSError(
                domain: "LivePodcastRAGAdapter",
                code: 5,
                userInfo: [NSLocalizedDescriptionKey: envelope.error ?? "Rust transcript hit projection failed"]
            )
        }
        return result.map(\.hit)
    }

    private static func rawKnowledgeRow(_ row: KnowledgeQueryRow) -> [String: Any] {
        [
            "episode_id": row.episodeId,
            "podcast_id": row.podcastId,
            "episode_title": row.episodeTitle,
            "podcast_title": row.podcastTitle,
            "chunk_index": row.chunkIndex,
            "start_secs": row.startSecs,
            "end_secs": row.endSecs,
            "text": row.text,
            "relevance_score": row.relevanceScore,
        ]
    }

    private func searchTool(payload: [String: Any]) async throws -> EpisodeRollupEnvelope {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw NSError(
                domain: "LivePodcastRAGAdapter",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "kernel handle unavailable"]
            )
        }
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else {
            throw NSError(
                domain: "LivePodcastRAGAdapter",
                code: 3,
                userInfo: [NSLocalizedDescriptionKey: "Could not encode episode rollup request"]
            )
        }
        let response = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":"kernel handle unavailable"}"#
            }
            return json.withCString { ptr -> String in
                guard let result = nmp_app_podcast_agent_search_tool(handle, ptr) else {
                    return #"{"error":"null response from nmp_app_podcast_agent_search_tool"}"#
                }
                defer { nmp_free_string(result) }
                return String(cString: result)
            }
        }.value
        guard let responseData = response.data(using: .utf8),
              let envelope = try? JSONDecoder().decode(EpisodeRollupEnvelope.self, from: responseData)
        else {
            throw NSError(
                domain: "LivePodcastRAGAdapter",
                code: 4,
                userInfo: [NSLocalizedDescriptionKey: "Could not decode episode rollup response"]
            )
        }
        return envelope
    }

    private func transcriptHitTool(payload: [String: Any]) async throws -> TranscriptHitsEnvelope {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw NSError(
                domain: "LivePodcastRAGAdapter",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "kernel handle unavailable"]
            )
        }
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else {
            throw NSError(
                domain: "LivePodcastRAGAdapter",
                code: 3,
                userInfo: [NSLocalizedDescriptionKey: "Could not encode transcript hit request"]
            )
        }
        let response = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":"kernel handle unavailable"}"#
            }
            return json.withCString { ptr -> String in
                guard let result = nmp_app_podcast_agent_search_tool(handle, ptr) else {
                    return #"{"error":"null response from nmp_app_podcast_agent_search_tool"}"#
                }
                defer { nmp_free_string(result) }
                return String(cString: result)
            }
        }.value
        guard let responseData = response.data(using: .utf8),
              let envelope = try? JSONDecoder().decode(TranscriptHitsEnvelope.self, from: responseData)
        else {
            throw NSError(
                domain: "LivePodcastRAGAdapter",
                code: 4,
                userInfo: [NSLocalizedDescriptionKey: "Could not decode transcript hit response"]
            )
        }
        return envelope
    }

}
