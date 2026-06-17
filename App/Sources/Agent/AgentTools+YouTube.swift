import Foundation

// MARK: - YouTube tool handlers

extension AgentTools {
    private struct YouTubeSearchPlan: Decodable {
        let error: String?
        let query: String?
        let limit: Int?
    }

    private struct YouTubeIngestPlan: Decodable {
        let error: String?
        let url: String?
        let title: String?
        let transcribe: Bool?
    }

    static func searchYouTubeTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        let planEnvelope = await youtubeSearchPlanEnvelope(args: args)
        guard let planEnvelope,
              let planData = planEnvelope.data(using: .utf8),
              let plan = try? JSONDecoder().decode(YouTubeSearchPlan.self, from: planData)
        else { return toolError("YouTube search planning is unavailable") }
        if let error = plan.error { return toolError(error) }
        guard let query = plan.query, let limit = plan.limit else {
            return toolError("YouTube search plan was incomplete")
        }

        do {
            let results = try await deps.youtubeIngestion.searchVideos(query: query, limit: limit)
            return await youtubeSearchResultsEnvelope(query: query, results: results)
                ?? toolError("YouTube search result shaping is unavailable")
        } catch {
            return toolError("search_youtube failed: \(error.localizedDescription)")
        }
    }

    static func ingestYouTubeVideoTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await youtubeIngestPlan(args: args) else {
            return toolError("ingest_youtube_video planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let youtubeURL = plan.url else { return toolError("ingest_youtube_video plan was incomplete") }
        let transcribe = plan.transcribe ?? true

        do {
            let result = try await deps.youtubeIngestion.ingestVideo(
                youtubeURL: youtubeURL,
                customTitle: plan.title,
                transcribe: transcribe
            )
            var payload: [String: Any] = [
                "episode_id": result.episodeID,
                "title": result.title,
                "author": result.author,
                "transcribe": transcribe,
            ]
            if let dur = result.durationSeconds { payload["duration_seconds"] = dur }
            if let ts = result.transcriptStatus { payload["transcript_status"] = ts }
            return await actionTool(op: "youtube_ingest_result", payload: payload)
                ?? toolError("ingest_youtube_video result shaping is unavailable")
        } catch {
            return toolError("ingest_youtube_video failed: \(error.localizedDescription)")
        }
    }

    private static func youtubeIngestPlan(args: [String: Any]) async -> YouTubeIngestPlan? {
        guard let envelope = await actionTool(op: "youtube_ingest_plan", payload: args),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(YouTubeIngestPlan.self, from: data)
    }

    private static func youtubeSearchPlanEnvelope(args: [String: Any]) async -> String? {
        await youtubeSearchFFI(payload: ["args": args], op: "plan")
    }

    private static func youtubeSearchResultsEnvelope(
        query: String,
        results: [YouTubeSearchResult]
    ) async -> String? {
        let rows = results.map { result -> [String: Any] in
            var row: [String: Any] = [
                "url": result.url,
                "title": result.title,
                "author": result.author,
            ]
            if let duration = result.durationSeconds { row["duration_seconds"] = duration }
            return row
        }
        return await youtubeSearchFFI(
            payload: [
                "query": query,
                "results": rows,
            ],
            op: "results"
        )
    }

    private static func youtubeSearchFFI(
        payload: [String: Any],
        op: String
    ) async -> String? {
        let handleBits = await MainActor.run {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }
        guard let handleBits,
              let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return nil
            }
            return json.withCString { ptr in
                let result: UnsafeMutablePointer<CChar>?
                switch op {
                case "plan":
                    result = nmp_app_podcast_agent_youtube_search_plan(handle, ptr)
                case "results":
                    result = nmp_app_podcast_agent_youtube_search_results(handle, ptr)
                default:
                    result = nil
                }
                guard let result else { return nil }
                defer { nmp_free_string(result) }
                return String(cString: result)
            }
        }.value
    }
}
